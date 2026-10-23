use solx::parser::{ast::Stmt, parse};

#[test]
fn import_syntax_supports_default_names_aliases_and_existing_as_identifiers() {
    let program =
        parse(r#"import "math" import "./lib/player.solx" as player var as = 1"#).unwrap();
    assert_eq!(
        program.statements[0],
        Stmt::Import {
            specifier: "math".into(),
            name: "math".into()
        }
    );
    assert_eq!(
        program.statements[1],
        Stmt::Import {
            specifier: "./lib/player.solx".into(),
            name: "player".into()
        }
    );
    let program = parse(r#"import "./lib/player.solx""#).unwrap();
    assert!(matches!(&program.statements[0], Stmt::Import { name, .. } if name == "player"));
    parse(r#"import "./my module.solx" as module"#).unwrap();
}

#[test]
fn invalid_imports_report_the_offending_source_position() {
    for (source, message, column) in [
        ("import", "expected a string literal after 'import'", 7),
        ("import 1", "expected a string literal after 'import'", 8),
        ("import name", "expected a string literal after 'import'", 8),
        (
            "import \"math\" as",
            "expected a module name after 'as'",
            17,
        ),
        (
            "import \"math\" as 1",
            "expected a module name after 'as'",
            18,
        ),
        (
            "import \"\"",
            "import specifier must not be empty or contain NUL",
            8,
        ),
        (
            "import \"x-y.solx\"",
            "module name is not an identifier; use 'as name'",
            8,
        ),
        (
            "import \" math.solx\"",
            "module name is not an identifier; use 'as name'",
            8,
        ),
        (
            "import \"end.solx\"",
            "module name is not an identifier; use 'as name'",
            8,
        ),
    ] {
        let error = parse(source).unwrap_err();
        assert_eq!(error.message, message, "{source}");
        assert_eq!(
            (error.span.line, error.span.column),
            (1, column),
            "{source}"
        );
    }
    assert!(parse("import \"bad\0path.solx\" as bad").is_err());
}

#[test]
fn imports_work_inside_functions_and_loops_without_changing_control_flow_grammar() {
    parse(
        r#"
        fn make()
            import "math"
            for i = 1, 2 do
                import "string" as text
                if i == 2 then return fn(x) return text.upper(x) end end
            end
        end
    "#,
    )
    .unwrap();
    assert!(parse(r#"var value = import "math""#).is_err());
}
