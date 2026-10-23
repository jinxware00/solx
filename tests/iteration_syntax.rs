use solx::lexer::{TokenKind, tokenize};
use solx::limits::{ExecutionLimits, LimitKind, ParseLimits};
use solx::parser::ast::{Expr, Program, Stmt};
use solx::parser::{parse, parse_with_limits};
use solx::{interpreter::Interpreter, runtime::error::RuntimeErrorKind};

#[test]
fn in_is_a_keyword_and_longer_identifiers_remain_identifiers() {
    let tokens = tokenize("in index inside input").unwrap();
    assert_eq!(tokens[0].kind, TokenKind::In);
    for (token, name) in tokens[1..4].iter().zip(["index", "inside", "input"]) {
        assert_eq!(token.kind, TokenKind::Identifier(name.into()));
    }
    assert!(parse("var in = 1").is_err());
    parse("var record = { 'in' = 1 } record['in'] = 2").unwrap();
}

#[test]
fn collection_for_parses_one_or_two_names_and_preserves_full_source_spans() {
    for (source, second) in [
        ("for value in source() do continue end", None),
        ("for value, item in source() do continue end", Some("item")),
    ] {
        let program = parse(source).unwrap();
        let Stmt::Located { span, .. } = &program.statements[0] else {
            panic!("expected span");
        };
        assert_eq!((span.start, span.end), (0, source.len()));
        let Stmt::CollectionFor {
            name,
            second_name,
            iterable,
            body,
        } = program.statements[0].unlocated()
        else {
            panic!("expected collection for");
        };
        assert_eq!(name, "value");
        assert_eq!(second_name.as_deref(), second);
        assert!(matches!(iterable, Expr::Call { .. }));
        assert_eq!(body[0].unlocated(), &Stmt::Continue);
    }
    parse("for value in [] do;;;end for key, value in {} do end").unwrap();
}

#[test]
fn malformed_headers_identify_the_offending_token() {
    for (source, message, token) in [
        (
            "for in [] do end",
            "expected a loop variable after 'for'",
            "in",
        ),
        (
            "for x, in [] do end",
            "expected a second loop variable after ','",
            "in",
        ),
        (
            "for x, x in [] do end",
            "duplicate loop variable 'x'",
            "x in",
        ),
        (
            "for x, y, z in [] do end",
            "collection for supports at most two loop variables",
            ", z",
        ),
        (
            "for x, y = 1, 2 do end",
            "expected 'in' after collection loop variables",
            "=",
        ),
        (
            "for x: Number in [] do end",
            "expected '=' or 'in' after for loop variable",
            ":",
        ),
        ("for x in do end", "expected expression, found Do", "do"),
        (
            "for x in [], {} do end",
            "expected 'do' after for loop collection",
            ",",
        ),
        (
            "for x in [] end",
            "expected 'do' after for loop collection",
            "end",
        ),
    ] {
        let error = parse(source).unwrap_err();
        assert_eq!(error.message, message, "{source}");
        assert_eq!(error.span.start, source.find(token).unwrap(), "{source}");
    }
    let error = parse("for x in [] do\n").unwrap_err();
    assert_eq!(error.message, "expected 'end' to close for loop");
    assert_eq!((error.span.line, error.span.column), (2, 1));
}

#[test]
fn collection_loop_control_cannot_cross_a_function_boundary() {
    for keyword in ["break", "continue"] {
        for source in [
            format!("for x in [] do fn bad() {keyword} end end"),
            format!("for k, v in {{}} do var bad = fn() {keyword} end end"),
            format!("for x in [] do end {keyword}"),
        ] {
            assert_eq!(
                parse(&source).unwrap_err().message,
                format!("{keyword} is only allowed inside a loop")
            );
        }
    }
    assert_eq!(
        parse("for x in [] do return end").unwrap_err().message,
        "return is only allowed inside a function"
    );
    parse("fn f() for x in [] do fn inner() for k in {} do break end end continue end return end")
        .unwrap();
}

#[test]
fn collection_loops_obey_parser_nesting_and_host_ast_depth_limits() {
    let source = format!("{}{}", "for x in [] do ".repeat(30), "end ".repeat(30));
    let error = parse_with_limits(
        &source,
        ParseLimits {
            max_nesting: 12,
            ..ParseLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.limit, Some(LimitKind::ParseNesting));
    for in_body in [false, true] {
        let mut deep = Expr::Nil;
        for _ in 0..20 {
            deep = Expr::List(vec![deep]);
        }
        let (iterable, body) = if in_body {
            (Expr::List(vec![]), vec![Stmt::Expression(deep)])
        } else {
            (deep, vec![])
        };
        let mut interpreter = Interpreter::with_limits(ExecutionLimits {
            parse: ParseLimits {
                max_ast_depth: 12,
                ..ParseLimits::default()
            },
            ..ExecutionLimits::default()
        })
        .unwrap();
        let error = interpreter
            .run_program(&Program {
                statements: vec![Stmt::CollectionFor {
                    name: "x".into(),
                    second_name: None,
                    iterable,
                    body,
                }],
            })
            .unwrap_err();
        assert_eq!(
            error.kind,
            RuntimeErrorKind::LimitExceeded(LimitKind::AstDepth)
        );
    }
}
