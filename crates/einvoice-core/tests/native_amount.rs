use einvoice_core::ubl::ubl_model::Amount;
use rust_decimal::Decimal;

#[test]
fn native_amount_exposes_exact_decimal_arithmetic() {
    let value = Decimal::from_str_exact("9007199254740993.01").unwrap();
    let amount = Amount {
        value: Some(value),
        currency_id: Some("EUR".to_owned()),
    };
    let stored: Decimal = amount.value.unwrap();
    assert_eq!(
        stored + Decimal::new(1, 2),
        Decimal::from_str_exact("9007199254740993.02").unwrap()
    );
}
