use solx::interpreter::Interpreter;
use solx::runtime::{RuntimeError, Value};

const MAX_BYTES: usize = 1024 * 1024;

fn setup(values: Vec<Value>) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source("import 'string' global host = []")
        .unwrap();
    let Value::List(ref slot) = interpreter.get_global("host").unwrap() else {
        unreachable!()
    };
    *slot.borrow_mut() = values;
    interpreter
}

fn call(function: &str, values: Vec<Value>) -> Result<Value, RuntimeError> {
    let arguments = (1..=values.len())
        .map(|index| format!("host[{index}]"))
        .collect::<Vec<_>>()
        .join(",");
    let mut interpreter = setup(values);
    interpreter.run_source(&format!("global result = string.{function}({arguments})"))?;
    Ok(interpreter.get_global("result").unwrap())
}

fn text(value: &str) -> Value {
    Value::String(value.into())
}

fn failure(result: Result<Value, RuntimeError>) -> RuntimeError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected a string utility runtime error"),
    }
}

#[test]
fn replace_is_literal_left_to_right_and_does_not_rescan_replacements() {
    for (source, needle, replacement, expected) in [
        ("aaaaa", "aa", "x", "xxa"),
        ("ababa", "aba", "x", "xba"),
        ("aa", "a", "aa", "aaaa"),
        ("a.*b.*c", ".*", "$&\\1", "a$&\\1b$&\\1c"),
        ("é🙂é", "é", "", "🙂"),
        ("🙂🙂", "🙂", "界", "界界"),
        ("abc", "X", "z", "abc"),
        ("", "x", "y", ""),
        ("a\0a", "\0", "\n", "a\na"),
    ] {
        assert_eq!(
            call(
                "replace",
                vec![text(source), text(needle), text(replacement)]
            )
            .unwrap(),
            text(expected)
        );
    }
    assert_eq!(
        failure(call("replace", vec![text("abc"), text(""), text("x")])).message,
        "string.replace() argument 2 must not be empty"
    );
}

#[test]
fn byte_length_is_exact_and_scalar_apis_keep_their_existing_meaning() {
    for (source, bytes) in [("", 0), ("abc", 3), ("aé🙂", 7), ("e\u{301}", 3), ("\0", 1)] {
        assert!(
            matches!(call("byte_len", vec![text(source)]).unwrap(), Value::Integer(value) if value == bytes)
        );
    }
    let mut interpreter = setup(vec![text("aé🙂e\u{301}")]);
    interpreter
        .run_source(
            "global size = string.len(host[1]) \
         global piece = string.slice(host[1], 2, 2) \
         global position = string.find(host[1], '🙂')",
        )
        .unwrap();
    assert_eq!(interpreter.get_global("size"), Some(Value::Integer(5)));
    assert_eq!(interpreter.get_global("piece"), Some(text("é🙂")));
    assert_eq!(interpreter.get_global("position"), Some(Value::Integer(3)));
}

#[test]
fn codepoints_return_exact_scalars_and_round_trip_without_normalization() {
    let source = "aé🙂e\u{301}\0\r\n";
    let expected = [97, 233, 128578, 101, 769, 0, 13, 10];
    let value = call("codepoints", vec![text(source)]).unwrap();
    let Value::List(items) = &value else {
        unreachable!()
    };
    assert_eq!(items.borrow().len(), expected.len());
    for (value, expected) in items.borrow().iter().zip(expected) {
        assert!(matches!(value, Value::Integer(actual) if *actual == expected));
    }
    assert_eq!(call("from_codepoints", vec![value]).unwrap(), text(source));
    assert_eq!(
        call("codepoints", vec![text("")]).unwrap(),
        Value::list(vec![])
    );
    assert_eq!(
        call("from_codepoints", vec![Value::list(vec![])]).unwrap(),
        text("")
    );
}

#[test]
fn from_codepoints_accepts_integral_numbers_and_all_valid_scalar_boundaries() {
    assert_eq!(
        call(
            "from_codepoints",
            vec![Value::list(vec![
                Value::Integer(0),
                Value::Number(65.0),
                Value::Integer(0xd7ff),
                Value::Integer(0xe000),
                Value::Integer(0xffff),
                Value::Integer(0x10ffff),
            ])]
        )
        .unwrap(),
        text("\0A\u{d7ff}\u{e000}\u{ffff}\u{10ffff}")
    );
    for value in [
        Value::Integer(-1),
        Value::Integer(0xd800),
        Value::Integer(0xdfff),
        Value::Integer(0x110000),
        Value::Integer(i64::MAX),
        Value::Number(1.5),
        Value::Number(f64::INFINITY),
        Value::Number(f64::NAN),
        Value::Number(1e100),
    ] {
        let error = failure(call(
            "from_codepoints",
            vec![Value::list(vec![Value::Integer(65), value])],
        ));
        assert_eq!(
            error.message,
            "string.from_codepoints() argument 1 contains an invalid Unicode scalar at index 2; expected an integer in 0..1114111 excluding 55296..57343"
        );
        assert!(error.is_catchable());
    }
}

#[test]
fn arity_and_types_report_qualified_names_and_argument_positions() {
    for (function, count) in [
        ("byte_len", 1),
        ("codepoints", 1),
        ("from_codepoints", 1),
        ("replace", 3),
    ] {
        for values in [vec![], vec![Value::Nil; 4]] {
            let actual = values.len();
            assert_eq!(
                failure(call(function, values)).message,
                format!("string.{function}() expects {count} argument(s), got {actual}")
            );
        }
    }
    for function in ["byte_len", "codepoints"] {
        assert_eq!(
            failure(call(function, vec![Value::Integer(1)])).message,
            format!("string.{function}() argument 1 requires String, got Number")
        );
    }
    for index in 0..3 {
        let mut values = vec![text("a"), text("b"), text("c")];
        values[index] = Value::Boolean(false);
        assert_eq!(
            failure(call("replace", values)).message,
            format!(
                "string.replace() argument {} requires String, got Boolean",
                index + 1
            )
        );
    }
    assert_eq!(
        failure(call("from_codepoints", vec![text("A")])).message,
        "string.from_codepoints() argument 1 requires List, got String"
    );
    assert_eq!(
        failure(call("from_codepoints", vec![Value::list(vec![Value::Nil])])).message,
        "string.from_codepoints() argument 1 contains Nil at index 1; expected Number"
    );
}

#[test]
fn from_codepoints_handles_host_borrows_without_mutation_and_recovers() {
    let value = Value::list(vec![Value::Integer(65), Value::Integer(0x1f642)]);
    let Value::List(items) = &value else {
        unreachable!()
    };
    let mut interpreter = setup(vec![value.clone()]);
    {
        let _guard = items.borrow_mut();
        assert_eq!(
            interpreter
                .run_source("string.from_codepoints(host[1])")
                .unwrap_err()
                .message,
            "string.from_codepoints(): collection is mutably borrowed"
        );
    }
    {
        let _guard = items.borrow();
        interpreter
            .run_source("global result = string.from_codepoints(host[1])")
            .unwrap();
    }
    assert_eq!(interpreter.get_global("result"), Some(text("A🙂")));
    items.borrow_mut().push(Value::Integer(0xd800));
    assert!(
        interpreter
            .run_source("string.from_codepoints(host[1])")
            .is_err()
    );
    assert_eq!(items.borrow().len(), 3);
    items.borrow_mut().pop();
    interpreter
        .run_source("global result = string.from_codepoints(host[1])")
        .unwrap();
    assert_eq!(interpreter.get_global("result"), Some(text("A🙂")));
}

#[test]
fn replacement_preflights_growth_but_allows_large_inputs_that_shrink() {
    let at_limit = call(
        "replace",
        vec![text(&"a".repeat(MAX_BYTES / 2)), text("a"), text("aa")],
    )
    .unwrap();
    let Value::String(at_limit) = &at_limit else {
        unreachable!()
    };
    assert_eq!(at_limit.len(), MAX_BYTES);
    assert_eq!(
        failure(call(
            "replace",
            vec![text(&"a".repeat(MAX_BYTES / 2 + 1)), text("a"), text("aa")]
        ))
        .message,
        "string.replace() output exceeds 1048576 UTF-8 bytes"
    );
    assert_eq!(
        call(
            "replace",
            vec![text(&"a".repeat(MAX_BYTES + 1)), text("a"), text("")]
        )
        .unwrap(),
        text("")
    );
    assert_eq!(
        call(
            "replace",
            vec![
                text("abc"),
                text("missing"),
                text(&"x".repeat(MAX_BYTES + 1))
            ]
        )
        .unwrap(),
        text("abc")
    );
    assert_eq!(
        failure(call(
            "replace",
            vec![text("aa"), text("a"), text(&"é".repeat(MAX_BYTES / 2))]
        ))
        .message,
        "string.replace() output exceeds 1048576 UTF-8 bytes"
    );
}

#[test]
fn unicode_conversions_enforce_entry_and_utf8_byte_boundaries() {
    let at_limit = call("codepoints", vec![text(&"a".repeat(MAX_BYTES))]).unwrap();
    let Value::List(items) = &at_limit else {
        unreachable!()
    };
    assert_eq!(items.borrow().len(), MAX_BYTES);
    drop(at_limit);
    assert_eq!(
        failure(call("codepoints", vec![text(&"a".repeat(MAX_BYTES + 1))])).message,
        "string.codepoints() output exceeds 1048576 code points"
    );

    let values = vec![Value::Integer(0x1f642); MAX_BYTES / 4];
    let at_limit = call("from_codepoints", vec![Value::list(values.clone())]).unwrap();
    let Value::String(at_limit) = &at_limit else {
        unreachable!()
    };
    assert_eq!(at_limit.len(), MAX_BYTES);
    let mut too_many = values;
    too_many.push(Value::Integer(0x1f642));
    assert_eq!(
        failure(call("from_codepoints", vec![Value::list(too_many)])).message,
        "string.from_codepoints() output exceeds 1048576 UTF-8 bytes"
    );
    assert_eq!(
        failure(call(
            "from_codepoints",
            vec![Value::list(vec![Value::Integer(0); MAX_BYTES + 1])]
        ))
        .message,
        "string.from_codepoints() output exceeds 1048576 UTF-8 bytes"
    );
}
