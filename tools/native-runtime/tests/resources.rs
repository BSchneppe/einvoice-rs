use einvoice_native_runtime::{
    Arithmetic, AtomicType as T, Error, Failure, Limits, Resource, Scope,
};
fn limits() -> Limits {
    Limits::new(
        4096,
        4096,
        4096,
        1_000_000_000,
        1_000_000,
        1_000_000,
        1_000_000,
        128,
        1_000_000,
    )
    .unwrap()
}
fn resource<TT: std::fmt::Debug>(result: einvoice_native_runtime::Result<TT>) -> Resource {
    match result.unwrap_err() {
        Error::Fatal(Failure::Resource(r)) => r,
        other => panic!("expected resource, got {other:?}"),
    }
}
#[test]
fn validates_limits_and_checks_live_capacity_before_numeric_allocation() {
    assert_eq!(
        resource(Limits::new(0, 512, 100, 1000, 1000, 1000, 1000, 10, 1000)),
        Resource::InvalidLimits
    );
    let l = Limits::new(100, 512, 100, 100_000, 100_000, 1000, 1000, 1, 1000).unwrap();
    let mut s = Scope::new(l);
    let a = s.lexical(T::Integer, "1").unwrap();
    let before = s.usage();
    assert_eq!(resource(s.lexical(T::Integer, "2")), Resource::LiveValues);
    assert_eq!(
        s.usage().numeric_work_remaining,
        before.numeric_work_remaining
    );
    drop(a);
    assert_eq!(s.usage().live_values, 0);
    assert!(s.lexical(T::Integer, "2").is_ok());
    let mut s =
        Scope::new(Limits::new(100, 512, 100, 100_000, 100_000, 1000, 1000, 10, 191).unwrap());
    let before = s.usage();
    assert_eq!(resource(s.lexical(T::Integer, "1")), Resource::LiveValues);
    assert_eq!(
        before.numeric_work_remaining,
        s.usage().numeric_work_remaining
    );
}
#[test]
fn input_string_and_output_capacity_precede_allocation_and_release_live_slots() {
    let mut s = Scope::new(Limits::new(4, 512, 100, 100_000, 100_000, 4, 100, 10, 10000).unwrap());
    assert_eq!(
        resource(s.lexical(T::String, "12345")),
        Resource::InputBytes
    );
    let x = s.lexical(T::String, "1234").unwrap();
    drop(x);
    assert_eq!(s.usage().live_bytes, 0);
    assert_eq!(resource(s.lexical(T::String, "1")), Resource::StringBytes);
    assert_eq!(s.usage().live_values, 0);
    let mut s =
        Scope::new(Limits::new(100, 512, 100, 100_000, 100_000, 1000, 5, 10, 10000).unwrap());
    let x = s.lexical(T::String, "abc").unwrap();
    let out = s.render(&x).unwrap();
    assert_eq!(out.as_str(), "abc");
    assert_eq!(s.usage().live_values, 2);
    drop(out);
    let before = s.usage();
    assert_eq!(resource(s.render(&x)), Resource::OutputBytes);
    assert_eq!(
        s.usage().string_bytes_remaining,
        before.string_bytes_remaining
    );
    assert_eq!(s.usage().live_values, 1);
}
#[test]
fn castable_and_number_do_not_swallow_resource_failures() {
    let mut s =
        Scope::new(Limits::new(100, 4096, 100, 10, 100_000, 1000, 1000, 10, 10000).unwrap());
    let x = s.lexical(T::Boolean, "true").unwrap();
    assert!(matches!(
        resource(s.castable(&x, T::Integer)),
        Resource::Numeric(_)
    ));
    assert_eq!(s.usage().numeric_work_remaining, 0);
    assert!(matches!(
        resource(s.castable(&x, T::Decimal)),
        Resource::Numeric(_)
    ));
    let mut s =
        Scope::new(Limits::new(100, 512, 100, 100_000, 100_000, 1, 1000, 10, 10000).unwrap());
    let x = s.lexical(T::Integer, "1").unwrap();
    assert_eq!(resource(s.number(Some(&x))), Resource::StringBytes);
}
#[test]
fn exhausted_runtime_fuel_stays_exhausted_and_independent_callers_reset() {
    let l = Limits::new(100, 512, 100, 100_000, 2, 1000, 1000, 10, 10000).unwrap();
    let mut s = Scope::new(l);
    assert_eq!(resource(s.lexical(T::String, "x")), Resource::Work);
    assert_eq!(s.usage().runtime_work_remaining, 0);
    assert_eq!(resource(s.lexical(T::Boolean, "true")), Resource::Work);
    let mut good = Scope::new(limits());
    let value = good.lexical(T::Integer, "1").unwrap();
    assert_eq!(good.usage().live_values, 1);
    let mut other = Scope::new(limits());
    assert!(matches!(
        other.duplicate(&value),
        Err(Error::Fatal(Failure::ForeignScope))
    ));
    assert!(matches!(
        other.number(Some(&value)),
        Err(Error::Fatal(Failure::ForeignScope))
    ));
    assert!(matches!(
        other.render(&value),
        Err(Error::Fatal(Failure::ForeignScope))
    ));
    drop(good); // Dropping the scope does not invalidate the value's retention lease.
    drop(value);
    assert!(other.lexical(T::Integer, "2").is_ok());
}
#[test]
fn numeric_scale_bits_and_cumulative_multiplication_refuse_honestly() {
    let mut s = Scope::new(
        Limits::new(100, 32, 16, 100_000_000, 100_000, 10000, 10000, 10, 10000).unwrap(),
    );
    assert!(matches!(
        resource(s.lexical(T::Decimal, "0.00000000000000001")),
        Resource::Numeric(einvoice_numeric::ResourceCause::Scale)
    ));
    let a = s.lexical(T::Integer, "9999").unwrap();
    let b = s.arithmetic(Arithmetic::Multiply, &a, &a).unwrap();
    assert!(matches!(
        resource(s.arithmetic(Arithmetic::Multiply, &b, &b)),
        Resource::Numeric(einvoice_numeric::ResourceCause::CoefficientBits)
    ));
    assert!(matches!(
        resource(s.round(&a, i64::MIN)),
        Resource::Numeric(einvoice_numeric::ResourceCause::Scale)
    ));
    let mut s =
        Scope::new(Limits::new(100, 4096, 4096, 4000, 100_000, 10000, 10000, 10, 10000).unwrap());
    let mut a = s.lexical(T::Integer, "9").unwrap();
    let mut failed = false;
    for _ in 0..32 {
        match s.arithmetic(Arithmetic::Multiply, &a, &a) {
            Ok(next) => a = next,
            Err(Error::Fatal(Failure::Resource(Resource::Numeric(_)))) => {
                failed = true;
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(failed);
    assert_eq!(s.usage().numeric_work_remaining, 0);
}

#[test]
fn checked_signed_division_policy_refuses_scale_overflow_before_kernel_allocation() {
    let mut s = Scope::new(
        Limits::new(
            100,
            128,
            i64::MAX as u64,
            1_000_000,
            1_000_000,
            1_000_000,
            1_000_000,
            10,
            10000,
        )
        .unwrap(),
    );
    let mut a = s.lexical(T::Decimal, "0.1").unwrap();
    let mut b = s.lexical(T::Decimal, "10").unwrap();
    for _ in 0..62 {
        a = s.arithmetic(Arithmetic::Multiply, &a, &a).unwrap();
        b = s.arithmetic(Arithmetic::Multiply, &b, &b).unwrap();
    }
    let before = s.usage();
    assert_eq!(
        resource(s.arithmetic(Arithmetic::Divide, &a, &b)),
        Resource::SizeOverflow
    );
    assert_eq!(
        s.usage().numeric_work_remaining,
        before.numeric_work_remaining
    );
    assert_eq!(s.usage().live_values, 2);
}

#[test]
fn finite_binary_conversion_resource_is_not_a_cast_failure_or_nan() {
    let mut s = Scope::new(
        Limits::new(
            100, 32, 16, 1_000_000, 1_000_000, 1_000_000, 1_000_000, 10, 10000,
        )
        .unwrap(),
    );
    let value = s.lexical(T::Double, "0.1").unwrap();
    assert!(matches!(
        resource(s.castable(&value, T::Decimal)),
        Resource::Numeric(_)
    ));
    let mut s = Scope::new(limits());
    let bad = s.lexical(T::String, "bad").unwrap();
    let before = s.usage();
    assert!(!s.castable(&bad, T::Integer).unwrap());
    assert!(s.usage().numeric_work_remaining < before.numeric_work_remaining);
    assert_eq!(s.usage().live_values, before.live_values);
    let duplicate = s.duplicate(&bad).unwrap();
    assert_eq!(s.usage().live_values, 2);
    drop(duplicate);
    assert_eq!(s.usage().live_values, 1);
}
