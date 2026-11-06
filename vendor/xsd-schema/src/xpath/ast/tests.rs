// ============================================================================
// Tests
// ============================================================================

use super::*;

#[test]
fn test_axis_direction() {
    assert!(Axis::Child.is_forward());
    assert!(Axis::Parent.is_reverse());
    assert!(Axis::Ancestor.is_reverse());
    assert!(Axis::Descendant.is_forward());
}

#[test]
fn test_name_test() {
    let any = NameTest::any();
    assert!(any.prefix.is_none());
    assert!(any.local_name.is_none());

    let qname = NameTest::qname("xs".to_string(), "integer".to_string());
    assert_eq!(qname.prefix, Some("xs".to_string()));
    assert_eq!(qname.local_name, Some("integer".to_string()));
}

#[test]
fn test_value_node() {
    let s = ValueNode::String("hello".to_string());
    match s {
        ValueNode::String(v) => assert_eq!(v, "hello"),
        _ => panic!("Expected string"),
    }

    let i = ValueNode::Integer("42".to_string());
    match i {
        ValueNode::Integer(v) => assert_eq!(v, "42"),
        _ => panic!("Expected integer"),
    }
}

#[test]
fn test_abbrev_forward_step_default_axis() {
    // XPath 2.0 §3.2.4: the default axis of an abbreviated forward step is
    // `child` unless the node test is an AttributeTest or a
    // SchemaAttributeTest, in which case it is `attribute`.
    let span = SourceSpan::new(0, 1);

    let named = PathStepNode::abbrev_forward(NodeTest::Name(NameTest::any()), span);
    assert_eq!(named.axis, Axis::Child);

    let kind = PathStepNode::abbrev_forward(NodeTest::Kind(KindTest::AnyKind), span);
    assert_eq!(kind.axis, Axis::Child);

    let element = PathStepNode::abbrev_forward(
        NodeTest::Kind(KindTest::Element(ElementTest::default())),
        span,
    );
    assert_eq!(element.axis, Axis::Child);

    let attribute = PathStepNode::abbrev_forward(
        NodeTest::Kind(KindTest::Attribute(AttributeTest::default())),
        span,
    );
    assert_eq!(attribute.axis, Axis::Attribute);

    let schema_attribute = PathStepNode::abbrev_forward(
        NodeTest::Kind(KindTest::SchemaAttribute("id".to_string())),
        span,
    );
    assert_eq!(schema_attribute.axis, Axis::Attribute);
}
