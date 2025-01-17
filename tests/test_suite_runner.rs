#[cfg(test)]
mod tests {
    use einvoice::validate_invoice;
    use rstest::rstest;
    use std::borrow::Cow;
    use std::io::Read;
    use std::path::PathBuf;

    #[rstest]
    fn validate_each_ubl_file(#[files("tests/inputs/ubl/*.xml")] path: PathBuf) {
        // Read file to string with BufReader
        let mut reader = std::io::BufReader::new(std::fs::File::open(&path).unwrap());
        let mut original = String::new();
        reader
            .read_to_string(&mut original)
            .expect("Cannot read file");

        let invoice = validate_invoice(&original).unwrap();
        // Serialize
        let serialized = yaserde::ser::to_string(&invoice).unwrap();

        // Normalize both strings
        let normalized_original = normalize_xml(&original);
        let normalized_serialized = normalize_xml(&serialized);

        // Compare normalized strings
        assert_eq!(normalized_serialized, normalized_original);
    }

    #[rstest]
    fn validate_each_cii_file(#[files("tests/inputs/cii/*.xml")] path: PathBuf) {
        // Read file to string with BufReader
        let mut reader = std::io::BufReader::new(std::fs::File::open(&path).unwrap());
        let mut original = String::new();
        reader
            .read_to_string(&mut original)
            .expect("Cannot read file");

        let invoice = validate_invoice(&original).unwrap();
        // Serialize
        let serialized = yaserde::ser::to_string(&invoice).unwrap();

        // Normalize both strings
        let normalized_original = normalize_xml(&original);
        let normalized_serialized = normalize_xml(&serialized);

        // Compare normalized strings
        assert_eq!(normalized_serialized, normalized_original);
    }

    use regex::Regex;
    use xmltree::{Element, ParserConfig};

    fn normalize_xml(xml: &str) -> String {
        // Parse the XML string
        let element = Element::parse_with_config(xml.as_bytes(), ParserConfig::new())
            .expect("Failed to parse XML");

        // Create a String buffer
        let mut buffer = Vec::new();

        // Write the normalized XML to the buffer with newlines and indentation
        element
            .write_with_config(
                &mut buffer,
                xmltree::EmitterConfig {
                    perform_indent: true,           // Enable indentation
                    indent_string: Cow::from("  "), // Use two spaces for indentation
                    line_separator: Cow::from("\n".to_string()),
                    ..Default::default()
                },
            )
            .expect("Failed to write XML");

        // Convert the buffer to a String
        let string = String::from_utf8(buffer).expect("Failed to convert buffer to String");
        // replace ubl:Invoice with Invoice
        simplify_invoice(&string)
    }

    fn simplify_invoice(input: &str) -> String {
        // Regex to match and remove xmlns attributes
        let xmlns_re = Regex::new(r#" xmlns(:[a-zA-Z0-9_-]+)?="[^"]*""#).unwrap();

        // Replace opening tags to remove xmlns attributes
        let result = xmlns_re.replace_all(input, "").to_string();

        // Normalize opening tags to just <Invoice>
        result.replace("ubl:Invoice", "Invoice") // Remove ubl: prefix
    }
}
