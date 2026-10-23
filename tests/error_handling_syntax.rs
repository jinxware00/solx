use solx::parser::{ast::Stmt, parse};

#[test]
fn parses_try_and_catch_bodies_with_source_spans() {
    let source = "try\n  num('bad')\ncatch problem\n  print(problem.message)\nend";
    let program = parse(source).unwrap();
    let Stmt::Located { span, statement } = &program.statements[0] else {
        panic!("missing source span")
    };
    assert_eq!(&source[span.start..span.end], source);
    let Stmt::TryCatch {
        body,
        error_name,
        catch_body,
    } = statement.as_ref()
    else {
        panic!("expected try/catch")
    };
    assert_eq!(error_name, "problem");
    assert_eq!(body.len(), 1);
    assert_eq!(catch_body.len(), 1);
    assert!(matches!(body[0], Stmt::Located { .. }));
    assert!(matches!(catch_body[0], Stmt::Located { .. }));
}

#[test]
fn accepts_empty_nested_bodies_and_bare_returns_before_catch() {
    for source in [
        "try catch err end",
        "try ; catch err ; end",
        "try try catch inner end catch outer try catch nested end end",
        "fn f() try return catch err return end end",
        "fn f() try return\ncatch err return\nend end",
        "try if true then print(1) else print(2) end catch err end",
    ] {
        parse(source).unwrap_or_else(|error| panic!("{source}: {error}"));
    }
}

#[test]
fn malformed_handlers_report_the_offending_token() {
    for (source, message, line, column) in [
        ("try", "expected 'catch' after try body", 1, 4),
        ("try\nend", "expected 'catch' after try body", 2, 1),
        ("try\ncatch", "expected an error name after 'catch'", 2, 6),
        (
            "try\ncatch 42\nend",
            "expected an error name after 'catch'",
            2,
            7,
        ),
        (
            "try\ncatch err\n",
            "expected 'end' to close try/catch",
            3,
            1,
        ),
        (
            "try catch err catch other end",
            "expected 'end' to close try/catch",
            1,
            15,
        ),
        (
            "try if true then catch err end",
            "expected 'end' to close if statement",
            1,
            18,
        ),
        ("catch err end", "expected expression, found Catch", 1, 1),
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
fn try_does_not_create_a_loop_or_function_control_flow_target() {
    for source in [
        "try break catch err end",
        "try catch err continue end",
        "try return 1 catch err end",
        "try catch err return end",
        "while true do try fn f() break end catch err end end",
        "while true do try catch err fn f() continue end end end",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
    for source in [
        "while true do try break catch err continue end end",
        "fn f() try return 1 catch err return 2 end end",
        "for i = 1, 2 do try fn f() end continue catch err break end end",
    ] {
        parse(source).unwrap();
    }
}
