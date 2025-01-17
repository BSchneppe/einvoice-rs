
# einvoice-rs

The goal of this project is to implement
- XML parsing of CII and UBL e-invoices
- Validation / [Parsing](https://lexi-lambda.github.io/blog/2019/11/05/parse-don-t-validate/) of CII and UBL e-invoices
- Generation of XML for CII and UBL e-invoices
- visualization of CII and UBL e-invoices
- generation of PDFs for CII  e-invoices (zugferd)
- parsing of XMls embedded in PDFs (zugferd)
- provide cross-language and platform bindings for the above

> [!CAUTION]
> Early stage of development, not ready for production use


## License

This project is licensed under the terms described in the [LICENSE](LICENSE) file (MIT).

## Features

- Serialization and Deserialization of CII and UBL XML invoices (EN 16931)
- Java Binding based on kotlin works for jre 8 and above


## Roadmap

- Implement validation for Invoices
- Clean-Up generated files according to xsds / use stricter types
- add more language bindings


## Running Tests

To run tests, run the following command

```bash
  cargo test  
```
This will validate that we can deserialize and serialize all tests in the [xrechnung testsuite](https://github.com/itplr-kosit/xrechnung-testsuite/tree/master/src/test )

## Examples
### Java
```java
import com.schneppe.einvoice.InvoiceStandard;
import com.schneppe.einvoice.EinvoiceKt;

String xml = new String(Files.readAllBytes(path));
InvoiceStandard invoiceStandard = EinvoiceKt.validateInvoice(xml);
```
### Rust 

see [examples](examples) for rust examples


## Authors

- [Benedikt Schneppe](https://www.github.com/bschneppe)





