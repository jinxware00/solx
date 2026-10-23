use solx::{interpreter::Interpreter, runtime::Value};

fn evaluate(source: &str) -> Result<Value, solx::runtime::RuntimeError> {
    let mut interpreter = Interpreter::new();
    interpreter.run_source(&format!("global answer = {source}"))?;
    Ok(interpreter.get_global("answer").unwrap())
}

#[test]
fn integer_literals_conversions_and_arithmetic_are_exact() {
    for (source, expected) in [
        ("9007199254740993", 9_007_199_254_740_993),
        ("9007199254740992 + 1", 9_007_199_254_740_993),
        ("9007199254740993 - 9007199254740992", 1),
        ("3000000000 * 3000000000", 9_000_000_000_000_000_000),
        ("9223372036854775807", i64::MAX),
        ("-9223372036854775808", i64::MIN),
        ("-0009223372036854775808", i64::MIN),
        ("num(' -9223372036854775808 ')", i64::MIN),
        ("num('9007199254740993')", 9_007_199_254_740_993),
        ("2 ^ 62", 1i64 << 62),
        ("(-1) ^ 9223372036854775807", -1),
    ] {
        assert!(
            matches!(evaluate(source).unwrap(), Value::Integer(n) if n == expected),
            "{source}"
        );
    }
}

#[test]
fn overflow_zero_division_and_nonfinite_results_are_errors() {
    for (source, fragment) in [
        ("9223372036854775808", "64-bit"),
        ("-9223372036854775809", "64-bit"),
        ("9223372036854775807 + 1", "overflow"),
        ("-9223372036854775808 - 1", "overflow"),
        ("-(-9223372036854775808)", "overflow"),
        ("3037000500 * 3037000500", "overflow"),
        ("2 ^ 63", "overflow"),
        ("-9223372036854775808 // -1", "overflow"),
        ("1 / 0", "division by zero"),
        ("0 / 0", "division by zero"),
        ("1 // 0", "division by zero"),
        ("1 % 0", "division by zero"),
        ("1.0 / -0.0", "division by zero"),
        ("1e308 * 2", "finite"),
        ("(-1.0) ^ 0.5", "finite"),
        ("0 ^ -1", "finite"),
        ("1e999", "finite"),
        ("num('inf')", "finite"),
        ("num('NaN')", "finite"),
        ("1e-999", "underflow"),
        ("1e-300 * 1e-300", "underflow"),
        ("1e-300 / 1e300", "underflow"),
        ("2 ^ -1075", "underflow"),
    ] {
        let error = evaluate(source).unwrap_err();
        assert!(
            error.message.contains(fragment),
            "{source}: {}",
            error.message
        );
    }
}

#[test]
fn mixed_arithmetic_requires_exact_integer_conversion() {
    for source in [
        "9007199254740993 + 0.0",
        "9007199254740993 / 1",
        "9223372036854775807 * 1.0",
    ] {
        assert!(
            evaluate(source).unwrap_err().message.contains("precision"),
            "{source}"
        );
    }
    for (source, expected) in [
        ("3 / 2", 1.5),
        ("1 + 0.5", 1.5),
        ("9007199254740992 + 0.0", 9_007_199_254_740_992.0),
        ("1e-300 * 0.0", 0.0),
    ] {
        assert_eq!(evaluate(source).unwrap(), Value::Number(expected));
    }
}

#[test]
fn mixed_comparisons_never_round_the_integer() {
    for source in [
        "1 == 1.0",
        "-0.0 == 0",
        "0 > -0.5",
        "0 < 0.5",
        "9007199254740993 > 9007199254740992.0",
        "9007199254740993 ~= 9007199254740992.0",
        "9223372036854775807 < 9223372036854775808.0",
        "-9223372036854775808 == -9223372036854775808.0",
        "-9223372036854775807 > -9223372036854775808.0",
        "[1, {x=9007199254740992}] == [1.0, {x=9007199254740992.0}]",
    ] {
        assert_eq!(evaluate(source).unwrap(), Value::Boolean(true), "{source}");
    }
    assert_ne!(Value::Integer(i64::MAX), Value::Number(i64::MAX as f64));
    assert_ne!(Value::Integer(0), Value::Number(f64::NAN));
}

#[test]
fn floor_division_and_modulo_agree_for_all_signs() {
    for (a, b, q, r) in [
        (7, 3, 2, 1),
        (-7, 3, -3, 2),
        (7, -3, -3, -2),
        (-7, -3, 2, -1),
    ] {
        assert_eq!(evaluate(&format!("{a} // {b}")).unwrap(), Value::Integer(q));
        assert_eq!(evaluate(&format!("{a} % {b}")).unwrap(), Value::Integer(r));
        assert_eq!(
            evaluate(&format!("{a}.0 % {b}.0")).unwrap(),
            Value::Integer(r)
        );
    }
    assert_eq!(
        evaluate("-9223372036854775808 % -1").unwrap(),
        Value::Integer(0)
    );
}

#[test]
fn loops_indices_annotations_and_native_operations_accept_exact_numbers() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
        import "math" import "list" import "string" import "core"
        global visits = []
        for i=9007199254740992,9007199254740994 do list.append(visits,i) end
        global edge=0
        for i=9223372036854775806,9223372036854775807,2 do edge=edge+1 end
        var large: Number = 9007199254740993
        global result=math.max(9007199254740992.0, large)
        global smallest=math.min(9223372036854775807,9223372036854775808.0)
        global absolute=math.abs(-9007199254740993)
        global rounded=math.floor(large)
        global text=str(large)
        global kind=core.type(large)
        var values=[9007199254740993,9007199254740992.0,1]
        list.sort(values)
        values[1]=2
        global sorted=values
        global length=#values
        global sliced=string.slice('abc',2,1)
    "#,
        )
        .unwrap();
    assert_eq!(
        interpreter.get_global("visits").unwrap().to_string(),
        "[9007199254740992, 9007199254740993, 9007199254740994]"
    );
    assert_eq!(interpreter.get_global("edge"), Some(Value::Integer(1)));
    for name in ["result", "absolute", "rounded"] {
        assert_eq!(
            interpreter.get_global(name),
            Some(Value::Integer(9_007_199_254_740_993))
        );
    }
    assert_eq!(
        interpreter.get_global("smallest"),
        Some(Value::Integer(i64::MAX))
    );
    assert_eq!(
        interpreter.get_global("text"),
        Some(Value::String("9007199254740993".into()))
    );
    assert_eq!(
        interpreter.get_global("kind"),
        Some(Value::String("Number".into()))
    );
    assert_eq!(
        interpreter.get_global("sorted").unwrap().to_string(),
        "[2, 9007199254740992, 9007199254740993]"
    );
    assert_eq!(interpreter.get_global("length"), Some(Value::Integer(3)));
    assert_eq!(
        interpreter.get_global("sliced"),
        Some(Value::String("b".into()))
    );
    assert!(
        interpreter
            .run_source("math.abs(-9223372036854775808)")
            .unwrap_err()
            .message
            .contains("overflow")
    );
}

#[test]
fn host_nonfinite_numbers_are_rejected_by_native_apis_without_mutation() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source("import 'math' import 'list' global values=[3,1,0]")
        .unwrap();
    let Value::List(values) = interpreter.get_global("values").unwrap() else {
        unreachable!()
    };
    for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        values.borrow_mut()[2] = Value::Number(value);
        for source in [
            "math.abs(values[3])",
            "math.max(1,values[3])",
            "list.sort(values)",
            "list.remove(values,values[3])",
        ] {
            assert!(
                interpreter
                    .run_source(source)
                    .unwrap_err()
                    .message
                    .contains("finite"),
                "{source}"
            );
        }
        assert_eq!(values.borrow()[0], Value::Integer(3));
        assert_eq!(values.borrow()[1], Value::Integer(1));
    }
}

#[test]
fn numeric_errors_are_catchable_and_restore_execution_state() {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(
            r#"
        global value=1
        try value=9223372036854775807+1 catch error global message=error.message end
        global after=value+1
    "#,
        )
        .unwrap();
    assert_eq!(interpreter.get_global("value"), Some(Value::Integer(1)));
    assert_eq!(interpreter.get_global("after"), Some(Value::Integer(2)));
    assert!(
        interpreter
            .get_global("message")
            .unwrap()
            .to_string()
            .contains("overflow")
    );
}
