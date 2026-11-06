use einvoice_numeric::{Budget, Decimal, Error, Limits, ResourceCause, RoundingMode as Mode};
use std::cmp::Ordering;

fn budget() -> Budget {
    Budget::new(Limits::new(4096, 65536, 8192, 16384, 1_000_000_000_000).unwrap())
}
fn parse(text: &str, b: &mut Budget) -> Decimal {
    Decimal::parse_decimal(text, b).unwrap()
}
fn text(value: &Decimal, b: &mut Budget) -> String {
    value.to_canonical(b).unwrap()
}

#[test]
fn normalized_equality_sign_and_sharing() {
    let mut b = budget();
    let a = parse("19", &mut b);
    for spelling in ["19.0", "+019.00", " \t019.000\r\n"] {
        assert_eq!(a, parse(spelling, &mut b));
    }
    let zero = parse("-000.000", &mut b);
    assert_eq!(zero.normalized_scale(), 0);
    assert_eq!(zero.sign(), 0);
    assert_eq!(zero, zero.negated());
    assert_eq!(zero, Decimal::zero(&mut b).unwrap());
    assert!(zero.is_integral());
    let negative = a.negated();
    assert_eq!(negative.sign(), -1);
    assert_eq!(negative.abs(), a);
    let before = b.remaining_work();
    let shared = negative.clone().negated().abs();
    assert_eq!(shared, a);
    assert_eq!(before, b.remaining_work());
    assert_eq!(parse("1900", &mut b).normalized_scale(), -2);
    assert!(!parse("1.9", &mut b).is_integral());
}

#[test]
fn unbounded_precision_within_budget_and_long_cancellation() {
    let mut b = budget();
    let integer = "123456789012345678901234567890123456789012345678901";
    let fraction = "123456789012345678901234567890123456789012345678901";
    let input = format!("{integer}.{fraction}");
    let a = parse(&input, &mut b);
    assert_eq!(text(&a, &mut b), input);
    let long = "9".repeat(1000);
    let a = parse(
        &format!("{long}.0000000000000000000000000000000000000001"),
        &mut b,
    );
    let integer = parse(&long, &mut b);
    let difference = a.sub(&integer, &mut b).unwrap();
    assert_eq!(
        text(&difference, &mut b),
        "0.0000000000000000000000000000000000000001"
    );
    assert!(a.sub(&a, &mut b).unwrap().is_zero());
}

#[test]
fn tax_product_stays_below_half() {
    let mut b = budget();
    let rate = parse("0.02631578947368421052631578947", &mut b);
    let nineteen = parse("19", &mut b);
    let product = rate.mul(&nineteen, &mut b).unwrap();
    assert_eq!(text(&product, &mut b), "0.49999999999999999999999999993");
    assert_eq!(
        product.compare(&parse(".5", &mut b), &mut b).unwrap(),
        Ordering::Less
    );
    assert!(product
        .quantize(0, Mode::HalfTowardPositiveInfinity, &mut b)
        .unwrap()
        .is_zero());
}

#[test]
fn all_signed_half_ties_and_nearest_directions() {
    let mut b = budget();
    let rows = [
        (Mode::HalfAwayFromZero, ["3", "-3", "4", "-4"]),
        (Mode::HalfTowardPositiveInfinity, ["3", "-2", "4", "-3"]),
        (Mode::HalfEven, ["2", "-2", "4", "-4"]),
        (Mode::HalfTowardZero, ["2", "-2", "3", "-3"]),
        (Mode::TowardZero, ["2", "-2", "3", "-3"]),
    ];
    for (mode, outputs) in rows {
        for (input, expected) in ["2.5", "-2.5", "3.5", "-3.5"].into_iter().zip(outputs) {
            let a = parse(input, &mut b);
            let rounded = a.quantize(0, mode, &mut b).unwrap();
            assert_eq!(text(&rounded, &mut b), expected, "{input} {mode:?}");
            let ten = parse("10", &mut b);
            let divided = parse(&input.replace('.', ""), &mut b)
                .rounded_divide(&ten, 0, mode, &mut b)
                .unwrap();
            assert_eq!(rounded, divided);
            let negative_ten = ten.negated();
            let divided = parse(&input.replace('.', ""), &mut b)
                .negated()
                .rounded_divide(&negative_ten, 0, mode, &mut b)
                .unwrap();
            assert_eq!(rounded, divided);
        }
        for (input, expected) in [
            ("2.4", "2"),
            ("-2.4", "-2"),
            ("2.6", if mode == Mode::TowardZero { "2" } else { "3" }),
            ("-2.6", if mode == Mode::TowardZero { "-2" } else { "-3" }),
        ] {
            let rounded = parse(input, &mut b).quantize(0, mode, &mut b).unwrap();
            assert_eq!(text(&rounded, &mut b), expected);
        }
    }
}

#[test]
fn signed_scales_and_explicit_division_policies() {
    let mut b = budget();
    for (a, d, scale, mode, expected) in [
        ("1", "8", 2, Mode::HalfAwayFromZero, "0.13"),
        ("1", "8", 2, Mode::HalfEven, "0.12"),
        ("-1", "8", 2, Mode::HalfTowardPositiveInfinity, "-0.12"),
        ("250", "1", -2, Mode::HalfAwayFromZero, "300"),
        ("250", "1", -2, Mode::HalfEven, "200"),
        ("12345", "2", -1, Mode::HalfTowardZero, "6170"),
        ("1", "300", 5, Mode::TowardZero, "0.00333"),
        ("0.01", "0.002", 0, Mode::TowardZero, "5"),
    ] {
        let value = parse(a, &mut b)
            .rounded_divide(&parse(d, &mut b), scale, mode, &mut b)
            .unwrap();
        assert_eq!(text(&value, &mut b), expected);
    }
    let a = parse("250", &mut b)
        .quantize(-2, Mode::HalfAwayFromZero, &mut b)
        .unwrap();
    assert_eq!(a.normalized_scale(), -2);
    assert_eq!(a.to_fixed(-2, &mut b).unwrap(), "300");
    let product = parse("100", &mut b)
        .mul(&parse("1000", &mut b), &mut b)
        .unwrap();
    assert_eq!(product.normalized_scale(), -5);
    assert_eq!(text(&product, &mut b), "100000");
}

#[test]
fn rendering_requires_explicit_rounding() {
    let mut b = budget();
    let a = parse("1.005", &mut b);
    assert_eq!(a.to_fixed(2, &mut b), Err(Error::InexactRendering));
    let a = a.quantize(2, Mode::HalfAwayFromZero, &mut b).unwrap();
    assert_eq!(a.to_fixed(2, &mut b).unwrap(), "1.01");
    assert_eq!(parse("19", &mut b).to_fixed(2, &mut b).unwrap(), "19.00");
    assert_eq!(
        parse("-0.001", &mut b).to_fixed(5, &mut b).unwrap(),
        "-0.00100"
    );
    assert_eq!(parse("0", &mut b).to_fixed(2, &mut b).unwrap(), "0.00");
    assert_eq!(
        parse("1000", &mut b).to_fixed(2, &mut b).unwrap(),
        "1000.00"
    );
    assert_eq!(
        parse("19", &mut b).to_fixed(-1, &mut b),
        Err(Error::InexactRendering)
    );
}

#[test]
fn builder_rounds_product_over_base_once() {
    let mut b = budget();
    let quantity = parse("3", &mut b);
    let price = parse("0.335", &mut b);
    let base = parse("2", &mut b);
    let product = quantity.mul(&price, &mut b).unwrap();
    assert_eq!(text(&product, &mut b), "1.005");
    let amount = product
        .rounded_divide(&base, 2, Mode::HalfAwayFromZero, &mut b)
        .unwrap();
    assert_eq!(amount, parse("0.50000", &mut b));
    assert_eq!(amount.to_fixed(2, &mut b).unwrap(), "0.50");
    let twice = product
        .quantize(2, Mode::HalfAwayFromZero, &mut b)
        .unwrap()
        .rounded_divide(&base, 2, Mode::HalfAwayFromZero, &mut b)
        .unwrap();
    assert_eq!(twice.to_fixed(2, &mut b).unwrap(), "0.51");
    assert_ne!(amount, twice);
}

fn hundredths(coefficient: i64) -> String {
    format!(
        "{}{}.{:02}",
        if coefficient < 0 { "-" } else { "" },
        coefficient.abs() / 100,
        coefficient.abs() % 100
    )
}

#[test]
fn generated_signed_quotient_remainder_integer_identities() {
    let mut budget = budget();
    for a in -45..=45 {
        for b in [-7, -3, 2, 9] {
            // Independent i64 identity in hundredths: divisor is b/10.
            let q = a / (10 * b);
            let r = a - q * 10 * b;
            let dividend = parse(&hundredths(a), &mut budget);
            let divisor = parse(&hundredths(b * 10), &mut budget);
            let quotient = dividend.integer_quotient(&divisor, &mut budget).unwrap();
            let remainder = dividend.remainder(&divisor, &mut budget).unwrap();
            assert!(quotient.is_integral());
            assert_eq!(quotient, parse(&q.to_string(), &mut budget));
            assert_eq!(remainder, parse(&hundredths(r), &mut budget));
            let reconstructed = quotient
                .mul(&divisor, &mut budget)
                .unwrap()
                .add(&remainder, &mut budget)
                .unwrap();
            assert_eq!(reconstructed, dividend);
            assert_eq!(
                remainder
                    .abs()
                    .compare(&divisor.abs(), &mut budget)
                    .unwrap(),
                Ordering::Less
            );
            assert!(remainder.is_zero() || remainder.sign() == dividend.sign());
        }
    }
}

#[test]
fn near_integer_quotient_never_uses_rounded_division() {
    let mut b = budget();
    let a = parse("1.999999999999999999999999999999999999", &mut b);
    let two = parse("2", &mut b);
    assert!(a.integer_quotient(&two, &mut b).unwrap().is_zero());
    assert_eq!(a.remainder(&two, &mut b).unwrap(), a);
    assert_eq!(
        text(
            &a.rounded_divide(&two, 2, Mode::HalfEven, &mut b).unwrap(),
            &mut b
        ),
        "1"
    );
}

#[test]
fn lexical_grammar_is_checked() {
    let mut b = budget();
    for invalid in [
        "", "+", "-", ".", "+.", "1e2", "1E-2", "NaN", "INF", "-INF", "1_000", "1 0", "1.2.3",
        "--1", "+-1", "١", "１", "\u{a0}1", "1\u{a0}", "\u{b}1", "\u{c}1",
    ] {
        assert_eq!(
            Decimal::parse_decimal(invalid, &mut b),
            Err(Error::InvalidLexical),
            "{invalid:?}"
        );
    }
    for valid in [".5", "+.5", "5.", "-0", "  \t\r\n+001.2300\n"] {
        assert!(Decimal::parse_decimal(valid, &mut b).is_ok());
    }
    for invalid in ["1.0", ".5", "1.", "1e2"] {
        assert_eq!(
            Decimal::parse_integer(invalid, &mut b),
            Err(Error::InvalidLexical)
        );
    }
    assert_eq!(
        Decimal::parse_integer("\t-001900\r", &mut b).unwrap(),
        parse("-1900", &mut b)
    );
}

#[test]
fn deterministic_ordering_across_signs_and_scales() {
    let mut b = budget();
    let inputs = [
        "-100000000000000000000000000000000000000",
        "-20",
        "-0.001",
        "0",
        "0.000000000000000000000000000000001",
        "1",
        "19.00",
        "190",
        "200",
    ];
    let values: Vec<_> = inputs.into_iter().map(|s| parse(s, &mut b)).collect();
    for (i, a) in values.iter().enumerate() {
        for (j, other) in values.iter().enumerate() {
            assert_eq!(a.compare(other, &mut b).unwrap(), i.cmp(&j));
        }
    }
}

#[test]
fn zero_division_is_distinct() {
    let mut b = budget();
    let a = parse("1", &mut b);
    let zero = parse("0", &mut b);
    for numerator in [&a, &zero] {
        assert_eq!(
            numerator.integer_quotient(&zero, &mut b),
            Err(Error::DivideByZero)
        );
        assert_eq!(numerator.remainder(&zero, &mut b), Err(Error::DivideByZero));
        assert_eq!(
            numerator.rounded_divide(&zero, 2, Mode::HalfEven, &mut b),
            Err(Error::DivideByZero)
        );
    }
}

#[test]
fn validated_limits_never_disable_checks() {
    for limits in [
        Limits::new(0, 1, 1, 1, 1),
        Limits::new(1, 0, 1, 1, 1),
        Limits::new(1, 1, 0, 1, 1),
        Limits::new(1, 1, 1, 0, 1),
        Limits::new(1, 1, 1, 1, 0),
        Limits::new(usize::MAX, 1, 1, 1, 1),
        Limits::new(1, u64::MAX, 1, 1, 1),
        Limits::new(1, 1, u64::MAX, 1, 1),
    ] {
        assert!(matches!(
            limits,
            Err(Error::Resource(ResourceCause::InvalidLimits))
        ));
    }
}

#[test]
fn resource_preflights_and_cross_scope_operands() {
    let mut b = budget();
    let large = parse("99999999999999999999", &mut b);
    let tiny = parse(".00000000000000000001", &mut b);
    let one = parse("1", &mut b);
    let mut small = Budget::new(Limits::new(8, 32, 100, 8, 100_000).unwrap());
    let high = parse("2147483648", &mut b);
    let square = parse("9999999", &mut b);
    assert_eq!(
        high.add(&high, &mut small),
        Err(Error::Resource(ResourceCause::CoefficientBits))
    );
    assert_eq!(
        square.mul(&square, &mut small),
        Err(Error::Resource(ResourceCause::CoefficientBits))
    );
    assert_eq!(
        Decimal::parse_decimal("123456789", &mut small),
        Err(Error::Resource(ResourceCause::InputBytes))
    );
    assert_eq!(
        Decimal::parse_decimal("99999999", &mut small)
            .unwrap()
            .mul(&large, &mut small),
        Err(Error::Resource(ResourceCause::CoefficientBits))
    );
    assert_eq!(
        one.add(&tiny, &mut small),
        Err(Error::Resource(ResourceCause::CoefficientBits))
    );
    assert_eq!(
        one.compare(&tiny, &mut small),
        Err(Error::Resource(ResourceCause::CoefficientBits))
    );
    assert_eq!(
        one.rounded_divide(&tiny, 10, Mode::HalfEven, &mut small),
        Err(Error::Resource(ResourceCause::CoefficientBits))
    );
    assert_eq!(
        one.rounded_divide(&one, 100, Mode::HalfEven, &mut small),
        Err(Error::Resource(ResourceCause::CoefficientBits))
    );
    assert_eq!(
        large.to_canonical(&mut small),
        Err(Error::Resource(ResourceCause::CoefficientBits))
    );
    assert_eq!(
        one.to_fixed(9, &mut small),
        Err(Error::Resource(ResourceCause::OutputBytes))
    );
    assert_eq!(
        one.quantize(101, Mode::HalfEven, &mut small),
        Err(Error::Resource(ResourceCause::Scale))
    );
    assert_eq!(
        Decimal::parse_decimal(
            ".0000001",
            &mut Budget::new(Limits::new(20, 128, 5, 20, 10000).unwrap())
        ),
        Err(Error::Resource(ResourceCause::Scale))
    );
}

#[test]
fn cumulative_work_exhaustion_is_sticky_and_size_weighted() {
    let mut setup = budget();
    let one = parse("1", &mut setup);
    let large = parse("123456789012345678901234567890", &mut setup);
    let limits = Limits::new(4096, 65536, 8192, 16384, 2000).unwrap();
    let mut small = Budget::new(limits);
    for _ in 0..10 {
        assert_eq!(one.mul(&one, &mut small).unwrap(), one);
    }
    let mut large_scope = Budget::new(limits);
    assert_eq!(
        large.mul(&large, &mut large_scope),
        Err(Error::Resource(ResourceCause::Work))
    );
    assert_eq!(large_scope.remaining_work(), 0);
    assert_eq!(
        Decimal::zero(&mut large_scope),
        Err(Error::Resource(ResourceCause::Work))
    );
    let mut operations = 0;
    while one.add(&one, &mut small).is_ok() {
        operations += 1;
        assert!(operations < 1000);
    }
    assert!(operations > 1);
    assert_eq!(small.remaining_work(), 0);
}

#[test]
fn extreme_scales_are_checked_without_panics_or_huge_powers() {
    let mut b = Budget::new(Limits::new(4096, 65536, i64::MAX as u64, 16384, 100_000_000).unwrap());
    let one = parse("1", &mut b);
    assert_eq!(one.quantize(i64::MAX, Mode::HalfEven, &mut b).unwrap(), one);
    assert!(one
        .quantize(-i64::MAX, Mode::HalfAwayFromZero, &mut b)
        .unwrap()
        .is_zero());
    for mode in [
        Mode::HalfAwayFromZero,
        Mode::HalfTowardPositiveInfinity,
        Mode::HalfEven,
        Mode::HalfTowardZero,
        Mode::TowardZero,
    ] {
        assert!(one
            .rounded_divide(&one, -i64::MAX, mode, &mut b)
            .unwrap()
            .is_zero());
        assert!(one
            .negated()
            .rounded_divide(&one, -i64::MAX, mode, &mut b)
            .unwrap()
            .is_zero());
    }
    assert_eq!(
        one.quantize(i64::MIN, Mode::HalfEven, &mut b),
        Err(Error::Resource(ResourceCause::Scale))
    );
    assert_eq!(
        one.to_fixed(i64::MAX, &mut b),
        Err(Error::Resource(ResourceCause::OutputBytes))
    );
    assert!(matches!(
        one.rounded_divide(&one, i64::MAX, Mode::HalfEven, &mut b),
        Err(Error::Resource(
            ResourceCause::SizeOverflow | ResourceCause::CoefficientBits
        ))
    ));
    assert_eq!(
        one.rounded_divide(&one, i64::MIN, Mode::HalfEven, &mut b),
        Err(Error::Resource(ResourceCause::Scale))
    );
    assert!(one
        .mul(&Decimal::zero(&mut b).unwrap(), &mut b)
        .unwrap()
        .is_zero());
}

#[test]
fn normalization_cannot_cross_scale_limit() {
    let mut setup = budget();
    let a = parse("5", &mut setup);
    let divisor = parse(".5", &mut setup);
    let mut b = Budget::new(Limits::new(20, 128, 1, 20, 100_000).unwrap());
    // 5 * 2 = 10, scale -1 is allowed; 5 / .5 = 10 also normalizes.
    assert_eq!(
        a.rounded_divide(&divisor, 0, Mode::HalfEven, &mut b)
            .unwrap(),
        parse("10", &mut setup)
    );
    let ten = parse("10", &mut setup);
    assert_eq!(
        ten.mul(&ten, &mut b),
        Err(Error::Resource(ResourceCause::Scale))
    );
    let fifty = parse("50", &mut setup);
    assert_eq!(
        fifty.add(&fifty, &mut b),
        Err(Error::Resource(ResourceCause::Scale))
    );
}
