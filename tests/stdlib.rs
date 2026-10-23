use solx::interpreter::Interpreter;
use solx::runtime::Value;

const IMPORTS: &str =
    r#"import "core" import "math" import "string" import "list" import "object""#;

fn run(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(&format!("{IMPORTS}\n{source}"))
        .unwrap();
    interpreter
}

fn evaluate(expression: &str) -> Value {
    run(&format!("global result = {expression}"))
        .get_global("result")
        .unwrap()
}

#[test]
fn inspection_and_existing_conversions_cover_all_types() {
    for (expression, expected) in [
        ("nil", "Nil"),
        ("true", "Boolean"),
        ("1", "Number"),
        ("''", "String"),
        ("[]", "List"),
        ("{}", "Object"),
        ("fn() end", "Function"),
        ("num", "Function"),
        ("math.sqrt", "Function"),
        ("math", "Module"),
    ] {
        assert_eq!(
            evaluate(&format!("core.type({expression})")),
            Value::String(expected.into())
        );
    }
    assert_eq!(evaluate("num(' 12.5 ')"), Value::Number(12.5));
    assert_eq!(evaluate("num(3)"), Value::Number(3.0));
    assert_eq!(evaluate("str(nil)"), Value::String("nil".into()));
    assert!(
        run("")
            .run_source("num('inf')")
            .unwrap_err()
            .message
            .contains("finite")
    );
    assert_eq!(
        evaluate("str(math.sqrt)"),
        Value::String("<function math.sqrt>".into())
    );
}

#[test]
fn maths_has_documented_boundaries_and_rounding() {
    for (expression, expected) in [
        ("math.abs(-3.5)", 3.5),
        ("math.abs(0)", 0.0),
        ("math.sqrt(0)", 0.0),
        ("math.sqrt(9)", 3.0),
        ("math.floor(-1.2)", -2.0),
        ("math.ceil(-1.2)", -1.0),
        ("math.round(1.5)", 2.0),
        ("math.round(-1.5)", -2.0),
        ("math.round(1.4)", 1.0),
        ("math.min(3, -2, 7)", -2.0),
        ("math.max(3, -2, 7)", 7.0),
        ("math.min(4)", 4.0),
        ("math.max(4)", 4.0),
        ("math.pi", std::f64::consts::PI),
    ] {
        assert_eq!(
            evaluate(expression),
            Value::Number(expected),
            "{expression}"
        );
    }
}

#[test]
fn maths_rejects_invalid_domains_and_nonfinite_values() {
    for (expression, message) in [
        (
            "math.sqrt(-1)",
            "math.sqrt() argument 1 must be nonnegative",
        ),
        ("math.abs(1 / 0)", "division by zero"),
        ("math.sqrt(0 / 0)", "division by zero"),
        ("math.floor(-1 / 0)", "division by zero"),
        ("math.ceil(1 / 0)", "division by zero"),
        ("math.round(0 / 0)", "division by zero"),
        ("math.min(1, 0 / 0)", "division by zero"),
        ("math.max(1 / 0)", "division by zero"),
    ] {
        let mut interpreter = run("");
        assert_eq!(
            interpreter.run_source(expression).unwrap_err().message,
            message
        );
    }
}

#[test]
fn strings_use_unicode_scalars_and_literal_search() {
    for (expression, expected) in [
        ("string.len('aé🙂')", Value::Number(3.0)),
        ("string.len('')", Value::Number(0.0)),
        ("string.upper('Straße')", Value::String("STRASSE".into())),
        ("string.lower('ÉABC')", Value::String("éabc".into())),
        ("string.trim('　 text 　')", Value::String("text".into())),
        ("string.slice('aé🙂z', 2, 2)", Value::String("é🙂".into())),
        ("string.slice('', 1, 0)", Value::String("".into())),
        ("string.slice('abc', 4, 0)", Value::String("".into())),
        ("string.find('é🙂é', '🙂')", Value::Number(2.0)),
        ("string.find('abc', 'x')", Value::Nil),
        ("string.find('', '')", Value::Number(1.0)),
        ("string.starts_with('éabc', 'é')", Value::Boolean(true)),
        ("string.starts_with('abc', 'A')", Value::Boolean(false)),
        ("string.ends_with('abc', '')", Value::Boolean(true)),
        ("string.ends_with('', 'a')", Value::Boolean(false)),
    ] {
        assert_eq!(evaluate(expression), expected, "{expression}");
    }
}

#[test]
fn split_and_join_preserve_empty_fields_without_coercion() {
    for (expression, expected) in [
        ("string.join(string.split('::a::::', '::'), '|')", "|a||"),
        ("string.join([], ',')", ""),
        ("string.join(['é', '🙂'], '-')", "é-🙂"),
        ("string.join(['a', 'b'], '')", "ab"),
    ] {
        assert_eq!(evaluate(expression), Value::String(expected.into()));
    }
    assert_eq!(
        evaluate("string.split('', ',')"),
        Value::list(vec![Value::String("".into())])
    );
    assert_eq!(
        evaluate("string.split('abc', ',')"),
        Value::list(vec![Value::String("abc".into())])
    );
}

#[test]
fn string_range_and_separator_errors_are_precise() {
    for (expression, message) in [
        (
            "string.slice('abc', 0, 1)",
            "string.slice() argument 2 must be an integer in 1..4",
        ),
        (
            "string.slice('abc', 1.5, 1)",
            "string.slice() argument 2 must be an integer in 1..4",
        ),
        (
            "string.slice('abc', 1e100, 1)",
            "string.slice() argument 2 must be an integer in 1..4",
        ),
        (
            "string.slice('abc', 4, 1)",
            "string.slice() argument 3 must be an integer in 0..0",
        ),
        (
            "string.slice('abc', 1, -1)",
            "string.slice() argument 3 must be an integer in 0..3",
        ),
        ("string.slice('abc', 1, 0 / 0)", "division by zero"),
        (
            "string.split('abc', '')",
            "string.split() argument 2 must not be empty",
        ),
        (
            "string.join(['ok', 1], ',')",
            "string.join() argument 1 contains Number at index 2; expected String",
        ),
    ] {
        let mut interpreter = run("");
        assert_eq!(
            interpreter.run_source(expression).unwrap_err().message,
            message
        );
    }
}

#[test]
fn list_mutation_is_shared_and_nil_is_an_entry() {
    let interpreter = run(r#"
        const items = []
        var alias = items
        global appended = list.append(items, 3)
        list.append(items, nil)
        list.append(items, 1)
        global size = list.len(alias)
        global nilIndex = list.find(alias, nil)
        global removed = list.remove(alias, 1)
        global nilRemoved = list.remove(items, 1)
        global result = alias
        global missing = list.find(items, 9)
    "#);
    assert_eq!(interpreter.get_global("appended"), Some(Value::Nil));
    assert_eq!(interpreter.get_global("size"), Some(Value::Number(3.0)));
    assert_eq!(interpreter.get_global("nilIndex"), Some(Value::Number(2.0)));
    assert_eq!(interpreter.get_global("removed"), Some(Value::Number(3.0)));
    assert_eq!(interpreter.get_global("nilRemoved"), Some(Value::Nil));
    assert_eq!(
        interpreter.get_global("result"),
        Some(Value::list(vec![Value::Number(1.0)]))
    );
    assert_eq!(interpreter.get_global("missing"), Some(Value::Nil));
}

#[test]
fn list_search_uses_existing_equality_and_function_identity() {
    let interpreter = run(r#"
        fn f() end
        global functionIndex = list.find([num, f], f)
        global collectionIndex = list.find([[1], {x = 2}], {x = 2})
        global differentFunction = list.find([f], fn() end)
        global empty = list.find([], nil)
    "#);
    assert_eq!(
        interpreter.get_global("functionIndex"),
        Some(Value::Number(2.0))
    );
    assert_eq!(
        interpreter.get_global("collectionIndex"),
        Some(Value::Number(2.0))
    );
    assert_eq!(
        interpreter.get_global("differentFunction"),
        Some(Value::Nil)
    );
    assert_eq!(interpreter.get_global("empty"), Some(Value::Nil));
}

#[test]
fn list_sort_is_in_place_for_numbers_strings_and_empty_lists() {
    for (initial, expected) in [
        ("[3, -1, 3, 2]", "[-1, 2, 3, 3]"),
        ("['z', 'A', 'a']", "[A, a, z]"),
        ("[]", "[]"),
    ] {
        let interpreter = run(&format!(
            "var items = {initial} var alias = items global result = list.sort(items) global sorted = str(alias)"
        ));
        assert_eq!(interpreter.get_global("result"), Some(Value::Nil));
        assert_eq!(
            interpreter.get_global("sorted"),
            Some(Value::String(expected.into()))
        );
    }
}

#[test]
fn list_failures_do_not_partially_mutate_the_list() {
    let mut empty = run("");
    assert_eq!(
        empty.run_source("list.remove([], 1)").unwrap_err().message,
        "list.remove() argument 1 must not be empty"
    );
    for (initial, operation) in [
        ("[3, 1, 'x']", "list.sort(items)"),
        ("[true, false]", "list.sort(items)"),
        ("[fn() end]", "list.sort(items)"),
        ("[{}, {}]", "list.sort(items)"),
        ("[]", "list.remove(items, 1)"),
        ("[1, 2]", "list.remove(items, 0)"),
        ("[1, 2]", "list.remove(items, -1)"),
        ("[1, 2]", "list.remove(items, 1.5)"),
        ("[1, 2]", "list.remove(items, 1e100)"),
        ("[1, 2]", "list.remove(items, 0 / 0)"),
    ] {
        let mut interpreter = run(&format!("var items = {initial} global before = str(items)"));
        assert!(interpreter.run_source(operation).is_err(), "{operation}");
        interpreter.run_source("global after = str(items)").unwrap();
        assert_eq!(
            interpreter.get_global("before"),
            interpreter.get_global("after")
        );
    }
}

#[test]
fn object_inspection_and_removal_are_deterministic_and_shared() {
    let interpreter = run(r#"
        const data = { z = 3, a = nil, m = 2 }
        var alias = data
        global keys = string.join(object.keys(data), ",")
        global hasNil = object.has(data, "a")
        global removed = object.remove(alias, "m")
        global missing = object.remove(data, "absent")
        global hasRemoved = object.has(data, "m")
        global size = #data
        global empty = object.keys({})
    "#);
    assert_eq!(
        interpreter.get_global("keys"),
        Some(Value::String("a,m,z".into()))
    );
    assert_eq!(interpreter.get_global("hasNil"), Some(Value::Boolean(true)));
    assert_eq!(interpreter.get_global("removed"), Some(Value::Number(2.0)));
    assert_eq!(interpreter.get_global("missing"), Some(Value::Nil));
    assert_eq!(
        interpreter.get_global("hasRemoved"),
        Some(Value::Boolean(false))
    );
    assert_eq!(interpreter.get_global("size"), Some(Value::Number(2.0)));
    assert_eq!(interpreter.get_global("empty"), Some(Value::list(vec![])));
}

#[test]
fn every_native_function_checks_missing_arguments() {
    for (name, count) in [
        ("core.type", "1"),
        ("core.assert", "1 to 2"),
        ("core.error", "1"),
        ("math.abs", "1"),
        ("math.sqrt", "1"),
        ("math.floor", "1"),
        ("math.ceil", "1"),
        ("math.round", "1"),
        ("math.min", "at least 1"),
        ("math.max", "at least 1"),
        ("string.len", "1"),
        ("string.upper", "1"),
        ("string.lower", "1"),
        ("string.trim", "1"),
        ("string.slice", "3"),
        ("string.find", "2"),
        ("string.split", "2"),
        ("string.join", "2"),
        ("string.starts_with", "2"),
        ("string.ends_with", "2"),
        ("list.len", "1"),
        ("list.append", "2"),
        ("list.remove", "2"),
        ("list.find", "2"),
        ("list.sort", "1"),
        ("object.keys", "1"),
        ("object.has", "2"),
        ("object.remove", "2"),
    ] {
        let mut interpreter = run("");
        assert_eq!(
            interpreter
                .run_source(&format!("{name}()"))
                .unwrap_err()
                .message,
            format!("{name}() expects {count} argument(s), got 0")
        );
        if !count.starts_with("at least") {
            let error = interpreter
                .run_source(&format!("{name}(nil, nil, nil, nil)"))
                .unwrap_err();
            assert!(error.message.ends_with("argument(s), got 4"), "{error}");
        }
    }
}

#[test]
fn invalid_native_argument_types_include_function_and_position() {
    for (expression, expected) in [
        (
            "core.error(1)",
            "core.error() argument 1 requires String, got Number",
        ),
        (
            "core.assert(true, 1)",
            "core.assert() argument 2 requires String, got Number",
        ),
        (
            "math.sqrt('9')",
            "math.sqrt() argument 1 requires Number, got String",
        ),
        (
            "math.min(1, false)",
            "math.min() argument 2 requires Number, got Boolean",
        ),
        (
            "string.upper(1)",
            "string.upper() argument 1 requires String, got Number",
        ),
        (
            "string.lower(nil)",
            "string.lower() argument 1 requires String, got Nil",
        ),
        (
            "string.trim([])",
            "string.trim() argument 1 requires String, got List",
        ),
        (
            "string.len({})",
            "string.len() argument 1 requires String, got Object",
        ),
        (
            "string.slice('abc', '1', 1)",
            "string.slice() argument 2 requires Number, got String",
        ),
        (
            "string.slice('abc', 1, false)",
            "string.slice() argument 3 requires Number, got Boolean",
        ),
        (
            "string.find('abc', 1)",
            "string.find() argument 2 requires String, got Number",
        ),
        (
            "string.split('abc', nil)",
            "string.split() argument 2 requires String, got Nil",
        ),
        (
            "string.join('abc', ',')",
            "string.join() argument 1 requires List, got String",
        ),
        (
            "string.join([], nil)",
            "string.join() argument 2 requires String, got Nil",
        ),
        (
            "string.starts_with('abc', 1)",
            "string.starts_with() argument 2 requires String, got Number",
        ),
        (
            "string.ends_with('abc', 1)",
            "string.ends_with() argument 2 requires String, got Number",
        ),
        (
            "list.len({})",
            "list.len() argument 1 requires List, got Object",
        ),
        (
            "list.append(nil, 1)",
            "list.append() argument 1 requires List, got Nil",
        ),
        (
            "list.find('abc', 1)",
            "list.find() argument 1 requires List, got String",
        ),
        (
            "list.sort({})",
            "list.sort() argument 1 requires List, got Object",
        ),
        (
            "list.remove([1], '1')",
            "list.remove() argument 2 requires Number, got String",
        ),
        (
            "object.keys([])",
            "object.keys() argument 1 requires Object, got List",
        ),
        (
            "object.has({}, 1)",
            "object.has() argument 2 requires String, got Number",
        ),
        (
            "object.remove({}, 1)",
            "object.remove() argument 2 requires String, got Number",
        ),
    ] {
        let mut interpreter = run("");
        assert_eq!(
            interpreter.run_source(expression).unwrap_err().message,
            expected,
            "{expression}"
        );
    }
}

#[test]
fn core_helpers_use_truthiness_and_restore_nested_calls_on_error() {
    for expression in ["0", "''", "[]", "{}"] {
        assert_eq!(
            evaluate(&format!("core.type(core.assert({expression}))")),
            evaluate(&format!("core.type({expression})"))
        );
    }
    let mut interpreter = run(r#"
        var visible = "file"
        fn fail() var visible = "inner" core.error("deliberate") end
        fn outer() if true then fail() end end
    "#);
    assert_eq!(
        interpreter.run_source("outer()").unwrap_err().message,
        "deliberate"
    );
    interpreter.run_source("global restored = visible").unwrap();
    assert_eq!(
        interpreter.get_global("restored"),
        Some(Value::String("file".into()))
    );
    assert_eq!(
        interpreter
            .run_source("core.assert(nil)")
            .unwrap_err()
            .message,
        "assertion failed"
    );
    assert_eq!(
        interpreter
            .run_source("core.assert(false, 'reason')")
            .unwrap_err()
            .message,
        "assertion failed: reason"
    );
}

#[test]
fn namespaces_are_read_only_cached_first_class_values() {
    let mut interpreter = run(r#"
        import "math" as m
        global same = m == math
        const typed: Module = m
        const function: Function = m.sqrt
        global answer = function(16)
        global text = str(typed)
        global indexed = m["sqrt"](25)
    "#);
    assert_eq!(interpreter.get_global("same"), Some(Value::Boolean(true)));
    assert_eq!(interpreter.get_global("answer"), Some(Value::Number(4.0)));
    assert_eq!(interpreter.get_global("indexed"), Some(Value::Number(5.0)));
    assert_eq!(
        interpreter.get_global("text"),
        Some(Value::String("<module math>".into()))
    );
    for source in ["math.sqrt = 1", "math['sqrt'] = 1"] {
        assert_eq!(
            interpreter.run_source(source).unwrap_err().message,
            "module namespace is read-only"
        );
    }
    assert_eq!(
        interpreter.run_source("math.missing").unwrap_err().message,
        "module 'math' has no export 'missing'"
    );
    assert_eq!(
        interpreter.run_source("math[1]").unwrap_err().message,
        "module index must be a String"
    );
    assert_eq!(
        interpreter
            .run_source("object.remove(math, 'sqrt')")
            .unwrap_err()
            .message,
        "object.remove() argument 1 requires Object, got Module"
    );
}

#[test]
fn imports_do_not_register_globals_and_follow_lexical_binding_rules() {
    let mut interpreter = Interpreter::new();
    assert!(interpreter.run_source("math").is_err());
    interpreter
        .run_source(
            r#"
        var outside = 2
        fn make() import "math" return fn(x) return math.sqrt(x) end end
        global result = make()(9)
        var as = 4
        import "math" as outside
        global bound = outside.sqrt(as)
        import "math" as outside
    "#,
        )
        .unwrap();
    assert_eq!(interpreter.get_global("result"), Some(Value::Number(3.0)));
    assert_eq!(interpreter.get_global("bound"), Some(Value::Number(2.0)));
    assert!(interpreter.run_source("math").is_err());
    assert!(interpreter.get_global("sqrt").is_none());
    assert_eq!(
        interpreter
            .run_source(r#"const fixed = 1 import "math" as fixed"#)
            .unwrap_err()
            .message,
        "cannot redeclare constant 'fixed'"
    );
}

#[test]
fn native_arguments_evaluate_once_before_validation() {
    let mut interpreter = run(r#"
        global calls = 0
        fn next() calls = calls + 1 return calls end
    "#);
    assert_eq!(
        interpreter
            .run_source("math.sqrt(next(), next())")
            .unwrap_err()
            .message,
        "math.sqrt() expects 1 argument(s), got 2"
    );
    assert_eq!(interpreter.get_global("calls"), Some(Value::Number(2.0)));
}
