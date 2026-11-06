mod support;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use support::*;
use xsd_schema::namespace::context::NamespaceContextSnapshot;
use xsd_schema::validation::errors::ValidationError;
use xsd_schema::validation::*;

#[derive(Default)]
struct Stored {
    attempts: usize,
    errors: Vec<ValidationError>,
    warnings: Vec<ValidationWarning>,
    bytes: usize,
}

#[derive(Clone)]
struct Sink {
    state: Rc<RefCell<Stored>>,
    stop: Rc<Cell<bool>>,
    count: usize,
    bytes: usize,
}

impl Sink {
    fn new(count: usize, bytes: usize) -> Self {
        Self {
            state: Rc::default(),
            stop: Rc::default(),
            count,
            bytes,
        }
    }

    // Test mechanism only: charges the raw message's UTF-8 bytes. This is not
    // production retained-evidence accounting or a memory/RSS limit.
    fn reserve(&self, bytes: usize) -> bool {
        let mut state = self.state.borrow_mut();
        state.attempts += 1;
        let available = state.errors.len() + state.warnings.len() < self.count
            && state
                .bytes
                .checked_add(bytes)
                .is_some_and(|n| n <= self.bytes);
        if available {
            state.bytes += bytes;
        } else {
            self.stop.set(true);
        }
        available
    }
}

impl ValidationSink for Sink {
    fn on_error(&mut self, e: ValidationError) {
        if self.reserve(e.message.len()) {
            self.state.borrow_mut().errors.push(e);
        }
    }
    fn on_warning(&mut self, w: ValidationWarning) {
        if self.reserve(w.message.len()) {
            self.state.borrow_mut().warnings.push(w);
        }
    }
    fn stop_requested(&self) -> bool {
        self.stop.get()
    }
}

#[test]
fn finalization_stops_after_first_refusal_with_at_least_three_unresolved_idrefs() {
    let schema = schema(XSD);
    let validator = SchemaValidator::new(&schema, ValidationFlags::default());
    let (_, ordinary, _) = capture(&schema, IDREFS);
    assert_eq!(ordinary.len(), 3);
    assert!(ordinary.iter().all(|e| e.constraint == "cvc-id.1"));
    for (count, bytes, callbacks, retained) in [
        (0, usize::MAX, 1, 0),
        (1, usize::MAX, 2, 1),
        (usize::MAX, 0, 1, 0),
    ] {
        let sink = Sink::new(count, bytes);
        let saved = sink.clone();
        let mut rt = validator.start_run(sink);
        // Hook-based upstream test driver does not finalize. All references are
        // pending, and no callbacks should have occurred before end_validation.
        let outcome =
            drive_quick_xml_with(IDREFS.as_bytes(), &mut rt, &schema, &mut NoopHandler).unwrap();
        assert_eq!(outcome.root_validity, Some(SchemaValidity::Invalid));
        assert_eq!(saved.state.borrow().attempts, 0);
        let error = rt.end_validation().unwrap_err();
        assert_eq!(error.constraint, "validation-aborted");
        assert_eq!(rt.operational_failure().unwrap().message, error.message);
        assert_eq!(saved.state.borrow().attempts, callbacks);
        assert_eq!(saved.state.borrow().errors.len(), retained);
        if retained == 1 {
            let state = saved.state.borrow();
            assert_eq!(state.errors[0].message, ordinary[0].message);
            assert_eq!(state.errors[0].instance_path(), ordinary[0].instance_path());
        }
        // A cancelled runtime is permanently terminal, even if control clears.
        saved.stop.set(false);
        let interned_before = schema.name_table.len();
        rt.validate_element(
            "other",
            "",
            None,
            None,
            &NamespaceContextSnapshot::default(),
        );
        rt.validate_attribute("a", "", "bad");
        rt.validate_text("bad");
        rt.validate_whitespace(" ");
        rt.validate_end_of_attributes();
        rt.validate_end_element();
        assert!(rt.end_validation().is_err());
        assert_eq!(saved.state.borrow().attempts, callbacks);
        assert_eq!(schema.name_table.len(), interned_before);
    }
    // A fresh runtime over the same prepared schema remains healthy.
    let (result, errors, _) = capture(&schema, VALID);
    assert_eq!(result.root_validity, Some(SchemaValidity::Valid));
    assert!(errors.is_empty());
}

#[test]
fn central_multi_error_dispatch_and_pre_cancellation_stop_out_of_band() {
    let schema = schema(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="r"><xs:complexType><xs:attribute name="a" use="required"/><xs:attribute name="b" use="required"/><xs:attribute name="c" use="required"/></xs:complexType></xs:element></xs:schema>"#,
    );
    assert_eq!(capture(&schema, "<r/>").1.len(), 3);
    let validator = SchemaValidator::new(&schema, ValidationFlags::default());
    for pre_cancel in [false, true] {
        let sink = Sink::new(0, usize::MAX);
        sink.stop.set(pre_cancel);
        let saved = sink.clone();
        let mut rt = validator.start_run(sink);
        let result = drive_quick_xml("<r/>".as_bytes(), &mut rt, &schema);
        assert!(matches!(result, Err(DriveError::Validation(_))));
        assert_eq!(saved.state.borrow().attempts, usize::from(!pre_cancel));
        assert!(saved.state.borrow().errors.is_empty());
        assert_eq!(
            rt.operational_failure().unwrap().constraint,
            "validation-aborted"
        );
    }
}

#[test]
fn warning_refusal_is_terminal_and_never_manufactures_document_errors() {
    let schema = schema(XSD);
    let (_, errors, warnings) = capture(&schema, CAPACITY);
    assert!(errors.is_empty());
    assert_eq!(warnings.len(), 3);
    let validator = SchemaValidator::new(&schema, ValidationFlags::default());
    let sink = Sink::new(0, usize::MAX);
    let saved = sink.clone();
    let mut rt = validator.start_run(sink);
    assert!(drive_quick_xml(CAPACITY.as_bytes(), &mut rt, &schema).is_err());
    assert_eq!(saved.state.borrow().attempts, 1);
    assert!(saved.state.borrow().errors.is_empty());
    assert!(rt.operational_failure().is_some());
}

#[test]
fn direct_schema_warning_paths_honor_refusal() {
    let mut schema = schema(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="r"><xs:complexType/><xs:unique name="a"><xs:selector xpath="item"/><xs:field xpath="@id"/></xs:unique><xs:unique name="b"><xs:selector xpath="item"/><xs:field xpath="@id"/></xs:unique><xs:unique name="c"><xs:selector xpath="item"/><xs:field xpath="@id"/></xs:unique></xs:element></xs:schema>"#,
    );
    // The text-schema loader rejects invalid identity XPath eagerly. Public
    // programmatic arenas can still carry it; exercise that defensive direct
    // warning path with actual compilation failures, without a mock validator.
    for (_, constraint) in &mut schema.arenas.entries_mut().identity_constraints {
        constraint.selector.xpath = "item[1]".to_owned();
    }
    let (_, _, warnings) = capture(&schema, "<r/>");
    assert_eq!(warnings.len(), 3);
    assert!(warnings
        .iter()
        .all(|w| w.instance_path().is_none() && w.location.is_none()));
    let validator = SchemaValidator::new(&schema, ValidationFlags::default());
    let sink = Sink::new(0, usize::MAX);
    let saved = sink.clone();
    let mut rt = validator.start_run(sink);
    assert!(drive_quick_xml("<r/>".as_bytes(), &mut rt, &schema).is_err());
    assert_eq!(saved.state.borrow().attempts, 1);
    assert!(saved.state.borrow().errors.is_empty());
}

#[test]
fn explicit_abort_and_mid_document_cancellation_are_terminal() {
    let schema = schema(XSD);
    let validator = SchemaValidator::new(&schema, ValidationFlags::default());
    for explicit in [false, true] {
        let sink = Sink::new(usize::MAX, usize::MAX);
        let saved = sink.clone();
        let mut rt = validator.start_run(sink);
        rt.validate_element("n", "", None, None, &NamespaceContextSnapshot::default());
        rt.validate_end_of_attributes();
        if explicit {
            rt.abort_run("adapter cancellation");
        } else {
            saved.stop.set(true);
        }
        rt.validate_text("bad");
        rt.validate_end_element();
        assert!(rt.end_validation().is_err());
        assert_eq!(saved.state.borrow().attempts, 0);
        if explicit {
            assert_eq!(
                rt.operational_failure().unwrap().message,
                "adapter cancellation"
            );
        }
    }
}

#[test]
fn direct_unresolved_keyref_warning_path_honors_refusal() {
    let mut schema = schema(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"><xs:element name="r"><xs:complexType/><xs:key name="target"><xs:selector xpath="item"/><xs:field xpath="@id"/></xs:key><xs:keyref name="a" refer="target"><xs:selector xpath="item"/><xs:field xpath="@id"/></xs:keyref><xs:keyref name="b" refer="target"><xs:selector xpath="item"/><xs:field xpath="@id"/></xs:keyref><xs:keyref name="c" refer="target"><xs:selector xpath="item"/><xs:field xpath="@id"/></xs:keyref></xs:element></xs:schema>"#,
    );
    // Exercise the other defensive warning for public programmatic schemas.
    // The ordinary text loader requires refer to resolve during compilation.
    let missing = schema.name_table.add("missing");
    for (_, constraint) in &mut schema.arenas.entries_mut().identity_constraints {
        if let Some(refer) = &mut constraint.refer {
            refer.local_name = missing;
        }
    }
    assert_eq!(capture(&schema, "<r/>").2.len(), 3);
    let validator = SchemaValidator::new(&schema, ValidationFlags::default());
    let sink = Sink::new(0, usize::MAX);
    let saved = sink.clone();
    let mut rt = validator.start_run(sink);
    assert!(drive_quick_xml("<r/>".as_bytes(), &mut rt, &schema).is_err());
    assert_eq!(saved.state.borrow().attempts, 1);
    assert!(saved.state.borrow().errors.is_empty());
}
