use solx::parser::ast::{Expr, Stmt, UnaryOp};
use solx::parser::parse;

#[test]
fn parses_while_with_nested_control_flow() {
    let program = parse("while true do if false then break else continue end end").unwrap();
    let Stmt::While { condition, body } = &program.statements[0] else {
        panic!("expected while loop");
    };
    assert_eq!(*condition, Expr::Boolean(true));
    let Stmt::If {
        branches,
        else_branch,
    } = &body[0]
    else {
        panic!("expected nested if");
    };
    assert_eq!(branches, &[(Expr::Boolean(false), vec![Stmt::Break])]);
    assert_eq!(*else_branch, Some(vec![Stmt::Continue]));
}

#[test]
fn parses_numeric_for_with_optional_step() {
    for (source, expected_step) in [
        ("for i = 1, 5 do end", None),
        (
            "for i = 1, 5, -2 do end",
            Some(Expr::Unary {
                op: UnaryOp::Negate,
                operand: Box::new(Expr::Number("2".into())),
            }),
        ),
    ] {
        let program = parse(source).unwrap();
        let Stmt::NumericFor {
            name,
            start,
            limit,
            step,
            body,
        } = &program.statements[0]
        else {
            panic!("expected numeric for loop");
        };
        assert_eq!(name, "i");
        assert_eq!(*start, Expr::Number("1".into()));
        assert_eq!(*limit, Expr::Number("5".into()));
        assert_eq!(*step, expected_step);
        assert!(body.is_empty());
    }
}

#[test]
fn malformed_loop_headers_and_bodies_report_positions() {
    for (source, message, line, column) in [
        ("while", "expected expression, found Eof", 1, 6),
        ("while true", "expected 'do' after while condition", 1, 11),
        (
            "while true do\n",
            "expected 'end' to close while loop",
            2,
            1,
        ),
        ("for", "expected a loop variable after 'for'", 1, 4),
        (
            "for 1 = 1, 2 do end",
            "expected a loop variable after 'for'",
            1,
            5,
        ),
        ("for i", "expected '=' after for loop variable", 1, 6),
        ("for i =", "expected expression, found Eof", 1, 8),
        ("for i = 1", "expected ',' after for loop start", 1, 10),
        ("for i = 1,", "expected expression, found Eof", 1, 11),
        ("for i = 1, 2,", "expected expression, found Eof", 1, 14),
        ("for i = 1, 2", "expected 'do' after for loop bounds", 1, 13),
        (
            "for i = 1, 2 do\n",
            "expected 'end' to close for loop",
            2,
            1,
        ),
        (
            "for i = 1, 2 do else end",
            "expected 'end' to close for loop",
            1,
            17,
        ),
    ] {
        let error = parse(source).unwrap_err();
        assert_eq!(error.message, message, "{source}");
        assert_eq!(
            (error.span.line, error.span.column),
            (line, column),
            "{source}"
        );
    }
}

#[test]
fn loop_control_requires_a_loop_in_the_same_function() {
    for keyword in ["break", "continue"] {
        for source in [
            keyword.to_owned(),
            format!("if true then {keyword} end"),
            format!("while false do end\n{keyword}"),
            format!("for i = 1, 2 do end\n{keyword}"),
            format!("while false do fn bad() {keyword} end end"),
            format!("for i = 1, 2 do var bad = fn() {keyword} end end"),
            format!("fn f() for i = 1, 2 do end {keyword} end"),
        ] {
            let error = parse(&source).unwrap_err();
            assert_eq!(
                error.message,
                format!("{keyword} is only allowed inside a loop"),
                "{source}"
            );
            let offset = source.rfind(keyword).unwrap();
            assert_eq!(
                (error.span.start, error.span.end),
                (offset, offset + keyword.len()),
                "{source}"
            );
        }
    }
}

#[test]
fn nested_function_and_loop_contexts_are_restored() {
    parse(
        r#"
        fn outer()
            for i = 1, 3 do
                fn inner()
                    while false do continue end
                    return
                end
                var anonymous = fn()
                    for j = 1, 2 do break end
                    return
                end
                while false do break end
                continue
            end
            return
        end
        "#,
    )
    .unwrap();
    for source in ["while false do return end", "for i = 1, 2 do return end"] {
        assert_eq!(
            parse(source).unwrap_err().message,
            "return is only allowed inside a function"
        );
    }
}

#[test]
fn allows_empty_loop_bodies_and_semicolons() {
    parse(";;;while false do;;;end;;;for i = 2, 1 do;;;end;;;").unwrap();
}
