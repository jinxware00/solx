use std::collections::HashMap;

use solx::interpreter::Interpreter;
use solx::runtime::{RuntimeError, Value};

const MAX_BYTES: usize = 1024 * 1024;
const MAX_DEPTH: usize = 64;

fn setup(value: Value) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source("import 'json' global host = [nil]")
        .unwrap();
    let Value::List(ref slot) = interpreter.get_global("host").unwrap() else {
        unreachable!()
    };
    slot.borrow_mut()[0] = value;
    interpreter
}

fn call(name: &str, value: Value) -> Result<Value, RuntimeError> {
    let mut interpreter = setup(value);
    interpreter.run_source(&format!("global result = json.{name}(host[1])"))?;
    Ok(interpreter.get_global("result").unwrap())
}

fn parse(text: &str) -> Result<Value, RuntimeError> {
    call("parse", Value::String(text.into()))
}

fn stringify(value: Value) -> Result<String, RuntimeError> {
    let Value::String(ref mut text) = call("stringify", value)? else {
        unreachable!()
    };
    Ok(std::mem::take(text))
}

// Avoid printing a potentially megabyte-sized successful result on failure.
fn failure<T>(result: Result<T, RuntimeError>) -> RuntimeError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("expected a JSON runtime error"),
    }
}

#[test]
fn json_requires_an_import_and_exposes_cached_read_only_functions() {
    let mut interpreter = Interpreter::new();
    assert!(interpreter.run_source("json.parse('null')").is_err());
    interpreter
        .run_source(
            "import 'json' import 'json' as j global same = json == j \
             const parseJson: Function = j.parse global result = parseJson('null') \
             global text = j['stringify'](true)",
        )
        .unwrap();
    assert_eq!(interpreter.get_global("same"), Some(Value::Boolean(true)));
    assert_eq!(interpreter.get_global("result"), Some(Value::Nil));
    assert_eq!(
        interpreter.get_global("text"),
        Some(Value::String("true".into()))
    );
    assert!(interpreter.get_global("parse").is_none());
    assert_eq!(
        interpreter
            .run_source("json.parse = nil")
            .unwrap_err()
            .message,
        "module namespace is read-only"
    );
}

#[test]
fn exact_arity_precedes_types_and_arguments_evaluate_once() {
    let mut interpreter = setup(Value::Nil);
    for function in ["parse", "stringify"] {
        for (arguments, count) in [("", 0), ("nil, nil", 2)] {
            assert_eq!(
                interpreter
                    .run_source(&format!("json.{function}({arguments})"))
                    .unwrap_err()
                    .message,
                format!("json.{function}() expects 1 argument(s), got {count}")
            );
        }
    }
    for (value, kind) in [
        ("nil", "Nil"),
        ("true", "Boolean"),
        ("1", "Number"),
        ("[]", "List"),
        ("{}", "Object"),
        ("fn() end", "Function"),
        ("json", "Module"),
    ] {
        assert_eq!(
            interpreter
                .run_source(&format!("json.parse({value})"))
                .unwrap_err()
                .message,
            format!("json.parse() argument 1 requires String, got {kind}")
        );
    }
    interpreter
        .run_source("global calls = 0 fn next() calls = calls + 1 return calls end")
        .unwrap();
    assert!(
        interpreter
            .run_source("json.parse(next(), next())")
            .is_err()
    );
    assert_eq!(interpreter.get_global("calls"), Some(Value::Integer(2)));
}

#[test]
fn scalar_values_and_unicode_round_trip() {
    for (source, expected) in [
        ("null", Value::Nil),
        ("true", Value::Boolean(true)),
        ("false", Value::Boolean(false)),
        ("0", Value::Integer(0)),
        ("-42", Value::Integer(-42)),
        ("1.25", Value::Number(1.25)),
        ("1e100", Value::Number(1e100)),
        (r#""""#, Value::String(String::new())),
        (r#""é🙂""#, Value::String("é🙂".into())),
        (r#""\u00e9\ud83d\ude42""#, Value::String("é🙂".into())),
        (
            r#""\"\\\/\b\f\n\r\t\u0000""#,
            Value::String("\"\\/\u{8}\u{c}\n\r\t\0".into()),
        ),
    ] {
        let value = parse(source).unwrap();
        assert_eq!(value, expected, "{source}");
        assert_eq!(parse(&stringify(value).unwrap()).unwrap(), expected);
    }
    let Value::Number(zero) = parse("-0").unwrap() else {
        panic!("negative JSON zero must preserve its sign")
    };
    assert!(zero.is_sign_negative());
    assert_eq!(stringify(Value::Number(zero)).unwrap(), "-0.0");
}

#[test]
fn integer_json_preserves_all_signed_64_bit_values() {
    for number in [
        i64::MIN,
        -9_007_199_254_740_993,
        9_007_199_254_740_993,
        i64::MAX,
    ] {
        let source = number.to_string();
        assert!(matches!(parse(&source).unwrap(), Value::Integer(value) if value == number));
        assert_eq!(stringify(Value::Integer(number)).unwrap(), source);
    }
    for source in [
        "9223372036854775808",
        "18446744073709551615",
        "18446744073709551616",
        "-9223372036854775809",
        "-18446744073709551616",
    ] {
        assert_eq!(
            parse(source).unwrap_err().message,
            "json.parse(): integer is outside the signed 64-bit range",
            "{source}"
        );
    }
    assert!(matches!(parse("42.0").unwrap(), Value::Number(42.0)));
    assert!(matches!(parse("42e0").unwrap(), Value::Number(42.0)));
}

#[test]
fn floating_overflow_underflow_and_nonfinite_host_values_are_rejected() {
    for source in ["1e309", "-1e309", "1e999999"] {
        assert!(
            parse(source)
                .unwrap_err()
                .message
                .contains("finite Number range")
        );
    }
    for source in ["1e-999", "-1e-999", "0.0001e-999"] {
        assert!(
            parse(source)
                .unwrap_err()
                .message
                .contains("underflows to zero")
        );
    }
    assert_eq!(parse("0e-999").unwrap(), Value::Number(0.0));
    let Value::Number(smallest) = parse("5e-324").unwrap() else {
        unreachable!()
    };
    assert!(smallest > 0.0);
    for number in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        for value in [
            Value::Number(number),
            Value::list(vec![Value::Number(number)]),
        ] {
            assert_eq!(
                stringify(value).unwrap_err().message,
                "json.stringify(): cannot serialize a nonfinite Number"
            );
        }
    }
}

#[test]
fn finite_float_serialization_preserves_every_bit_on_parse() {
    let mut state = 0x1234_5678_9abc_def0_u64;
    let mut numbers = vec![f64::MAX, f64::MIN_POSITIVE, f64::from_bits(1), -0.0];
    for _ in 0..512 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let number = f64::from_bits(state);
        if number.is_finite() {
            numbers.push(number);
        }
    }
    for number in numbers {
        let text = stringify(Value::Number(number)).unwrap();
        let Value::Number(decoded) = parse(&text).unwrap() else {
            panic!("floating JSON changed representation: {text}");
        };
        assert_eq!(decoded.to_bits(), number.to_bits(), "{text}");
    }
}

#[test]
fn nested_objects_preserve_null_entries_and_sort_keys_recursively() {
    let source = r#"{"z":[null,true,{"é":"🙂","a":-2}],"empty":{},"a":null}"#;
    let value = parse(source).unwrap();
    assert_eq!(
        stringify(value.clone()).unwrap(),
        r#"{"a":null,"empty":{},"z":[null,true,{"a":-2,"é":"🙂"}]}"#
    );
    let mut interpreter = setup(value);
    interpreter
        .run_source(
            "import 'object' global has = object.has(host[1], 'a') \
             global size = #host[1].z global nullValue = host[1].a \
             global emptyList = json.stringify([])",
        )
        .unwrap();
    assert_eq!(interpreter.get_global("has"), Some(Value::Boolean(true)));
    assert_eq!(interpreter.get_global("size"), Some(Value::Integer(3)));
    assert_eq!(interpreter.get_global("nullValue"), Some(Value::Nil));
    assert_eq!(
        interpreter.get_global("emptyList"),
        Some(Value::String("[]".into()))
    );
}

#[test]
fn duplicate_object_keys_use_the_last_value() {
    assert_eq!(
        stringify(parse(r#"{"a":1,"a":null}"#).unwrap()).unwrap(),
        r#"{"a":null}"#
    );
    // Numeric validation also covers values discarded by a duplicate key.
    assert!(parse(r#"{"a":9223372036854775808,"a":null}"#).is_err());
}

#[test]
fn invalid_json_and_trailing_data_are_rejected() {
    for source in [
        "",
        " ",
        "nil",
        "NaN",
        "Infinity",
        "+1",
        "01",
        "1.",
        "[1,]",
        "{a:1}",
        r#"{"a":1,}"#,
        "true false",
        "{}[]",
        "1 trailing",
        "// comment\n1",
        "'text'",
        "\"line\nbreak\"",
        r#""\uD800""#,
        r#""\uDC00""#,
        r#""\x41""#,
    ] {
        let error = parse(source).unwrap_err();
        assert!(
            error.message.starts_with("json.parse(): invalid JSON:"),
            "{source}: {error}"
        );
        assert!(error.is_catchable());
    }
    assert_eq!(
        parse(" \t\r\n[1]\n ").unwrap(),
        Value::list(vec![Value::Integer(1)])
    );
}

#[test]
fn string_contents_do_not_count_as_syntax_or_numbers() {
    let text = format!(
        "{}9223372036854775808 \\\" {}",
        "[".repeat(100),
        "}".repeat(100)
    );
    let encoded = serde_json::to_string(&text).unwrap();
    assert_eq!(parse(&encoded).unwrap(), Value::String(text));
}

#[test]
fn unsupported_functions_and_modules_fail_even_when_nested() {
    let mut interpreter = setup(Value::Nil);
    for (source, kind) in [
        ("json", "Module"),
        ("fn() end", "Function"),
        ("print", "Function"),
        ("json.parse", "Function"),
        ("[json]", "Module"),
        ("{nested = [json.parse]}", "Function"),
    ] {
        assert_eq!(
            interpreter
                .run_source(&format!("json.stringify({source})"))
                .unwrap_err()
                .message,
            format!("json.stringify(): argument 1 contains unsupported {kind}")
        );
    }
}

#[test]
fn active_path_cycles_fail_but_shared_acyclic_children_serialize() {
    let mut interpreter = setup(Value::Nil);
    interpreter
        .run_source(
            "import 'list' global cycle = [] list.append(cycle, cycle) \
             global record = {} record.self = record \
             global mixed = [record] record.mixed = mixed",
        )
        .unwrap();
    for source in ["cycle", "record", "mixed"] {
        assert_eq!(
            interpreter
                .run_source(&format!("json.stringify({source})"))
                .unwrap_err()
                .message,
            "json.stringify(): cannot serialize cyclic collections"
        );
    }
    interpreter
        .run_source(
            "global shared = {x = [1, nil]} \
             global text = json.stringify([shared, shared, {again = shared}])",
        )
        .unwrap();
    assert_eq!(
        interpreter.get_global("text"),
        Some(Value::String(
            r#"[{"x":[1,null]},{"x":[1,null]},{"again":{"x":[1,null]}}]"#.into()
        ))
    );
}

#[test]
fn host_borrow_conflicts_are_errors_and_recover_without_mutation() {
    for value in [
        Value::list(vec![Value::Integer(1)]),
        Value::object(HashMap::from([("x".into(), Value::Integer(1))])),
    ] {
        let mut interpreter = setup(Value::list(vec![value.clone()]));
        let before = stringify(value.clone()).unwrap();
        let mut check = || {
            assert_eq!(
                interpreter
                    .run_source("json.stringify(host[1])")
                    .unwrap_err()
                    .message,
                "json.stringify(): collection is mutably borrowed"
            );
        };
        match &value {
            Value::List(items) => {
                let _guard = items.borrow_mut();
                check();
            }
            Value::Object(properties) => {
                let _guard = properties.borrow_mut();
                check();
            }
            _ => unreachable!(),
        }
        interpreter.run_source("json.stringify(host[1])").unwrap();
        assert_eq!(stringify(value.clone()).unwrap(), before);
        match &value {
            Value::List(items) => {
                let _guard = items.borrow();
                interpreter.run_source("json.stringify(host[1])").unwrap();
            }
            Value::Object(properties) => {
                let _guard = properties.borrow();
                interpreter.run_source("json.stringify(host[1])").unwrap();
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn depth_limits_accept_the_boundary_and_reject_deep_input_before_recursing() {
    let at_limit = format!("{}null{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
    assert_eq!(stringify(parse(&at_limit).unwrap()).unwrap(), at_limit);
    let objects_at_limit = format!(
        "{}null{}",
        "{\"x\":".repeat(MAX_DEPTH),
        "}".repeat(MAX_DEPTH)
    );
    assert_eq!(
        stringify(parse(&objects_at_limit).unwrap()).unwrap(),
        objects_at_limit
    );
    for depth in [MAX_DEPTH + 1, 100_000] {
        let text = format!("{}null{}", "[".repeat(depth), "]".repeat(depth));
        assert_eq!(
            parse(&text).unwrap_err().message,
            "json.parse(): nesting exceeds 64 containers"
        );
    }
    let mut nodes = vec![Value::Nil];
    for _ in 0..2_000 {
        nodes.push(Value::list(vec![nodes.last().unwrap().clone()]));
    }
    assert_eq!(
        stringify(nodes.last().unwrap().clone())
            .unwrap_err()
            .message,
        "json.stringify(): nesting exceeds 64 containers"
    );
    // Destruction of arbitrary host graphs is independent of JSON traversal.
    // Keep nodes alive while breaking their edges to avoid recursive host drop.
    for node in &nodes {
        if let Value::List(items) = node {
            items.borrow_mut().clear();
        }
    }
}

#[test]
fn utf8_input_and_escaped_output_limits_are_exact() {
    let text = "x".repeat(MAX_BYTES - 2);
    let encoded = format!("\"{text}\"");
    assert_eq!(parse(&encoded).unwrap(), Value::String(text.clone()));
    assert_eq!(stringify(Value::String(text)).unwrap().len(), MAX_BYTES);
    assert_eq!(
        failure(parse(&(encoded + " "))).message,
        "json.parse(): input exceeds 1048576 UTF-8 bytes"
    );
    for text in [
        "x".repeat(MAX_BYTES - 1),
        "é".repeat(MAX_BYTES / 2),
        "\0".repeat(MAX_BYTES / 6 + 1),
    ] {
        assert_eq!(
            failure(stringify(Value::String(text))).message,
            "json.stringify(): output exceeds 1048576 UTF-8 bytes"
        );
    }
}

#[test]
fn shared_graph_expansion_is_bounded_by_output_size() {
    let mut value = Value::list(vec![Value::Integer(0)]);
    for _ in 0..20 {
        value = Value::list(vec![value.clone(), value]);
    }
    assert_eq!(
        failure(stringify(value)).message,
        "json.stringify(): output exceeds 1048576 UTF-8 bytes"
    );
}

#[test]
fn json_errors_are_catchable_and_the_interpreter_recovers() {
    let mut interpreter = setup(Value::Nil);
    interpreter
        .run_source(
            "fn decode() return json.parse('true false') end \
             try decode() catch err global message = err.message end \
             global recovered = json.parse('true')",
        )
        .unwrap();
    let Value::String(ref message) = interpreter.get_global("message").unwrap() else {
        unreachable!()
    };
    assert!(message.starts_with("json.parse(): invalid JSON:"));
    assert_eq!(
        interpreter.get_global("recovered"),
        Some(Value::Boolean(true))
    );
}
