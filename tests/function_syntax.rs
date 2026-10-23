use solx::parser::ast::{Expr, Stmt};
use solx::parser::parse;

#[test]
fn named_and_anonymous_functions_share_parameter_and_body_structure() {
    let program = parse("fn add(a, b) return a + b end var empty = fn() return end").unwrap();
    let Stmt::Function { name, function } = program.statements[0].unlocated() else {
        panic!("expected named function declaration");
    };
    assert_eq!(name, "add");
    assert_eq!(function.parameters, ["a", "b"]);
    assert_eq!(function.body.len(), 1);
    assert!(matches!(function.body[0].unlocated(), Stmt::Return(values) if values.len() == 1));
    let Stmt::Declaration {
        initializer: Some(Expr::Function(function)),
        ..
    } = program.statements[1].unlocated()
    else {
        panic!("expected anonymous function initializer");
    };
    assert!(function.parameters.is_empty());
    assert_eq!(function.body.len(), 1);
    assert_eq!(function.body[0].unlocated(), &Stmt::Return(vec![]));
}

#[test]
fn parses_calls_on_anonymous_functions_and_returned_functions() {
    let program = parse("global result = (fn(x) return fn() return x end end)(42)()").unwrap();
    let Stmt::Declaration {
        initializer: Some(Expr::Call { callee, arguments }),
        ..
    } = program.statements[0].unlocated()
    else {
        panic!("expected call initializer");
    };
    assert!(arguments.is_empty());
    assert!(matches!(callee.as_ref(), Expr::Call { callee, arguments }
        if matches!(callee.as_ref(), Expr::Function(_)) && arguments.len() == 1));
}

#[test]
fn malformed_functions_report_specific_messages_and_source_positions() {
    for (source, message, line, column) in [
        ("fn", "expected a function name after 'fn'", 1, 3),
        ("fn 123() end", "expected a function name after 'fn'", 1, 4),
        (
            "fn add a end",
            "expected '(' before function parameters",
            1,
            8,
        ),
        (
            "fn add(a b) end",
            "expected ')' after function parameters",
            1,
            10,
        ),
        ("fn add(a,) end", "expected a parameter name", 1, 10),
        ("fn add(a,a) end", "duplicate parameter 'a'", 1, 10),
        ("fn add(1) end", "expected a parameter name", 1, 8),
        (
            "var f = fn named() end",
            "expected '(' before function parameters",
            1,
            12,
        ),
        ("fn f()\nreturn 1", "expected 'end' to close function", 2, 9),
        (
            "fn f()\nreturn 1 +",
            "expected expression, found Eof",
            2,
            11,
        ),
        ("fn f() else end", "expected 'end' to close function", 1, 8),
        (
            "fn f()\nif true then\nreturn 1\nend",
            "expected 'end' to close function",
            4,
            4,
        ),
        (
            "fn f()\nreturn 1, 2\nend",
            "multiple return values are not supported yet",
            2,
            9,
        ),
    ] {
        let error = parse(source).unwrap_err();
        assert_eq!(error.message, message, "{source}");
        assert_eq!(
            (error.span.line, error.span.column),
            (line, column),
            "{source}"
        );
        assert!(error.to_string().ends_with(&format!("at {line}:{column}")));
    }
}

#[test]
fn malformed_calls_report_source_positions_including_eof() {
    for (source, message, column) in [
        ("f(", "expected expression, found Eof", 3),
        ("f(1", "expected ')' after function arguments", 4),
        ("f(,)", "expected expression, found Comma", 3),
        ("f(1,)", "expected expression, found RightParen", 5),
        ("f(1 2)", "expected ')' after function arguments", 5),
    ] {
        let error = parse(source).unwrap_err();
        assert_eq!(error.message, message, "{source}");
        assert_eq!(
            (error.span.line, error.span.column),
            (1, column),
            "{source}"
        );
    }
}

#[test]
fn returns_outside_functions_are_rejected_even_after_nested_functions() {
    for (source, line) in [
        ("return 1", 1),
        ("if true then\nreturn\nend", 2),
        ("fn outer() fn inner() return end return end\nreturn", 2),
    ] {
        let error = parse(source).unwrap_err();
        assert_eq!(error.message, "return is only allowed inside a function");
        assert_eq!((error.span.line, error.span.column), (line, 1));
    }
}

#[test]
fn accepts_empty_statements_and_bare_returns_before_block_terminators() {
    parse(";;; fn empty();;;return;;end;;;empty();;;").unwrap();
    parse("fn choose(x) if x then return elseif false then return else return end end").unwrap();
    parse("fn bare() return\nvar unreachable = 1 end").unwrap();
}
