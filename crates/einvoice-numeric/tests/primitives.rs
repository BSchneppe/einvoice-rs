use einvoice_numeric::{Budget, Decimal, Error, Limits, Result, RoundingMode};

fn decode(hex: &str) -> String {
    String::from_utf8(
        hex.as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}

#[test]
fn selected_recorded_primitives() {
    let mut count = 0;
    for line in include_str!("fixtures/primitives.tsv").lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 9);
        let mut budget = Budget::new(Limits::new(4096, 65536, 8192, 16384, 100_000_000).unwrap());
        let a = decode(fields[2]);
        let b = decode(fields[3]);
        let mode = match fields[5] {
            "HalfTowardZero" => RoundingMode::HalfTowardZero,
            "HalfTowardPositiveInfinity" => RoundingMode::HalfTowardPositiveInfinity,
            "HalfEven" => RoundingMode::HalfEven,
            "TowardZero" => RoundingMode::TowardZero,
            _ => panic!("unknown fixture mode"),
        };
        let execute = |budget: &mut Budget| -> Result<String> {
            let integer = fields[1] == "imod";
            let parse = if integer {
                Decimal::parse_integer
            } else {
                Decimal::parse_decimal
            };
            let a = parse(&a, budget)?;
            let value = match fields[1] {
                "cast" => a,
                "round" | "even" => a.quantize(fields[4].parse().unwrap(), mode, budget)?,
                op => {
                    let b = parse(&b, budget)?;
                    match op {
                        "+" => a.add(&b, budget)?,
                        "-" => a.sub(&b, budget)?,
                        "*" => a.mul(&b, budget)?,
                        "idiv" => a.integer_quotient(&b, budget)?,
                        "mod" | "imod" => a.remainder(&b, budget)?,
                        "div" => a.rounded_divide(&b, fields[4].parse().unwrap(), mode, budget)?,
                        _ => panic!("unknown primitive"),
                    }
                }
            };
            value.to_canonical(budget)
        };
        let result = execute(&mut budget);
        let expected = decode(fields[7]);
        match fields[6] {
            "OK" => assert_eq!(result.unwrap(), expected, "{}", fields[0]),
            "ERROR" => {
                let error = match expected.as_str() {
                    "FORG0001" => Error::InvalidLexical,
                    "FOAR0001" => Error::DivideByZero,
                    _ => panic!("unexpected reference error"),
                };
                assert_eq!(result.unwrap_err(), error, "{}", fields[0]);
            }
            _ => panic!("unexpected reference status"),
        }
        count += 1;
    }
    assert_eq!(count, 57);
}
