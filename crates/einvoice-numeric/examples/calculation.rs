use einvoice_numeric::{Budget, Decimal, Limits, Result, RoundingMode};
use std::cmp::Ordering;

fn main() -> Result<()> {
    let mut budget = Budget::new(Limits::new(4096, 65536, 8192, 16384, 100_000_000)?);
    let quantity = Decimal::parse_decimal("3", &mut budget)?;
    let price = Decimal::parse_decimal("0.335", &mut budget)?;
    let base = Decimal::parse_decimal("2", &mut budget)?;
    let product = quantity.mul(&price, &mut budget)?;
    let amount = product.rounded_divide(&base, 2, RoundingMode::HalfAwayFromZero, &mut budget)?;
    let expected = Decimal::parse_decimal("0.50", &mut budget)?;
    assert_eq!(amount.compare(&expected, &mut budget)?, Ordering::Equal);
    println!("{}", amount.to_fixed(2, &mut budget)?);
    Ok(())
}
