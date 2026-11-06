use serde_json::{json, Value};
use xee_xpath::{context::StaticContextBuilder, Documents, Queries, Query};

fn main() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("../cases.json")).unwrap();
    let mut documents = Documents::new();
    let document = documents
        .add_string(
            "http://example.com/probe.xml".try_into().unwrap(),
            "<root xmlns:r='urn:probe'><r:a n='1'><r:b>42</r:b></r:a><r:a n='2'/></root>",
        )
        .unwrap();
    let mut context = StaticContextBuilder::default();
    context.add_namespace("r", "urn:probe");
    context.add_namespace("u", "utils");
    let queries = Queries::new(context);
    for case in cases {
        let expression = format!(
            "string-join(({}) ! string(.), '|')",
            case["expr"].as_str().unwrap()
        );
        let result = queries
            .one(&expression, |_, item| Ok(item.try_into_value::<String>()?))
            .and_then(|query| query.execute(&mut documents, document));
        println!(
            "{}",
            match result {
                Ok(value) => json!({"id": case["id"], "value": value, "error": false}),
                Err(error) =>
                    json!({"id": case["id"], "value": null, "error": true, "detail": format!("{error:?}")}),
            }
        );
    }
}
