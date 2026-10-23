use solx::interpreter::Interpreter;
use solx::parser::{ast::Stmt, parse};
use solx::runtime::Value;

#[test]
fn syntax_and_lexical_errors_retain_the_original_message_and_position() {
    for (source, message, line, column) in [
        (
            "var ok = 1\nvar broken = )",
            "expected expression, found RightParen",
            2,
            14,
        ),
        ("var ok = 1\n@", "unexpected character '@'", 2, 1),
    ] {
        let error = Interpreter::new().run_source(source).unwrap_err();
        assert_eq!(error.message, message);
        let location = error.location.as_ref().unwrap();
        assert!(location.file.is_none());
        let span = location.span.unwrap();
        assert_eq!((span.line, span.column), (line, column));
        assert!(
            error
                .to_string()
                .contains(&format!("at <source>:{line}:{column}"))
        );
        assert!(error.contexts.is_empty());
    }
}

#[test]
fn nested_calls_keep_innermost_statement_and_call_sites_after_source_is_dropped() {
    let mut interpreter = Interpreter::new();
    let source =
        String::from("fn inner()\n  return num('bad')\nend\nfn outer()\n  return inner()\nend");
    interpreter.run_source(&source).unwrap();
    drop(source);
    let error = interpreter.run_source("\nouter()").unwrap_err();
    assert_eq!(error.message, "cannot convert 'bad' to Number");
    let span = error.location.as_ref().unwrap().span.unwrap();
    assert_eq!((span.line, span.column), (2, 3));
    assert_eq!(error.contexts.len(), 2);
    assert_eq!(error.contexts[0].description, "in inner()");
    assert_eq!(error.contexts[1].description, "in outer()");
    let inner_call = error.contexts[0].location.as_ref().unwrap().span.unwrap();
    let outer_call = error.contexts[1].location.as_ref().unwrap().span.unwrap();
    assert_eq!((inner_call.line, inner_call.column), (5, 3));
    assert_eq!((outer_call.line, outer_call.column), (2, 1));
    let next = interpreter.run_source("num('next')").unwrap_err();
    assert!(next.contexts.is_empty());
    assert_eq!(next.location.unwrap().span.unwrap().line, 1);
    interpreter.run_source("global recovered = 42").unwrap();
    assert_eq!(
        interpreter.get_global("recovered"),
        Some(Value::Number(42.0))
    );
}

#[test]
fn nested_blocks_loops_and_arity_failures_use_the_failing_statement() {
    let mut interpreter = Interpreter::new();
    let error = interpreter
        .run_source("fn f(a) end\nwhile true do\n  if true then\n    f()\n  end\n  break\nend")
        .unwrap_err();
    assert_eq!(error.message, "f() expects 1 argument(s), got 0");
    let span = error.location.unwrap().span.unwrap();
    assert_eq!((span.line, span.column), (4, 5));
    let error = interpreter
        .run_source("for i = 1, 2, 0 do end")
        .unwrap_err();
    assert_eq!(error.location.unwrap().span.unwrap().column, 1);
}

#[test]
fn statement_metadata_survives_ast_cloning_and_covers_full_source_bytes() {
    let source = "-- comment\n  var name = 'é'\n";
    let program = parse(source).unwrap().clone();
    let Stmt::Located { span, statement } = &program.statements[0] else {
        panic!("missing span")
    };
    assert_eq!((span.line, span.column), (2, 3));
    assert_eq!(&source[span.start..span.end], "var name = 'é'");
    assert!(matches!(statement.as_ref(), Stmt::Declaration { .. }));
}
