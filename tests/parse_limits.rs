use solx::limits::{LimitKind, ParseLimits};
use solx::parser::{parse, parse_with_limits};

#[test]
fn nesting_limits_cover_all_recursive_syntax_and_malformed_input() {
    let limits = ParseLimits {
        max_nesting: 12,
        ..ParseLimits::default()
    };
    for source in [
        format!("{}1{}", "(".repeat(50), ")".repeat(50)),
        format!("{}true", "not ".repeat(50)),
        format!("{}1", "1 ^ ".repeat(50)),
        format!("{}'x'", "'x' .. ".repeat(50)),
        format!("{}1{}", "[".repeat(50), "]".repeat(50)),
        format!("{}1{}", "{x=".repeat(50), "}".repeat(50)),
        format!("{}{}", "if true then ".repeat(50), "end ".repeat(50)),
        format!("{}{}", "fn f() ".repeat(50), "end ".repeat(50)),
        format!("{}1{}", "fn() return ".repeat(50), " end".repeat(50)),
        format!("{}{}", "while true do ".repeat(50), "end ".repeat(50)),
        format!("{}{}", "try ".repeat(50), "catch e end ".repeat(50)),
        "(".repeat(50),
    ] {
        let error = parse_with_limits(&source, limits).unwrap_err();
        assert_eq!(
            error.limit,
            Some(LimitKind::ParseNesting),
            "{source}: {error}"
        );
        assert!(error.span.end <= source.len());
        assert!(error.span.line > 0 && error.span.column > 0);
    }
}

#[test]
fn ast_limits_cover_iterative_chains_and_partial_tree_cleanup() {
    let limits = ParseLimits {
        max_ast_depth: 12,
        ..ParseLimits::default()
    };
    for source in [
        format!("1{}", "+1".repeat(1000)),
        format!("x{}", ".x".repeat(1000)),
        format!("x{}", "[1]".repeat(1000)),
        format!("x{}", "()".repeat(1000)),
        format!("1{}+", "+1".repeat(1000)),
        format!("fn f() return 1{} end", "+1".repeat(1000)),
    ] {
        let error = parse_with_limits(&source, limits).unwrap_err();
        assert_eq!(error.limit, Some(LimitKind::AstDepth), "{error}");
    }
}

#[test]
fn boundary_counts_include_source_wrappers_and_report_the_statement() {
    let source = "-- position\n  var x = 1";
    parse_with_limits(
        source,
        ParseLimits {
            max_ast_depth: 3,
            ..ParseLimits::default()
        },
    )
    .unwrap();
    let error = parse_with_limits(
        source,
        ParseLimits {
            max_ast_depth: 2,
            ..ParseLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.limit, Some(LimitKind::AstDepth));
    assert_eq!((error.span.line, error.span.column), (2, 3));
    parse_with_limits(
        "1",
        ParseLimits {
            max_nesting: 2,
            ..ParseLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        parse_with_limits(
            "1",
            ParseLimits {
                max_nesting: 1,
                ..ParseLimits::default()
            }
        )
        .unwrap_err()
        .limit,
        Some(LimitKind::ParseNesting)
    );
}

#[test]
fn wide_and_sequential_programs_do_not_accumulate_depth() {
    parse(&"var x = [1, 2, 3]\n".repeat(2000)).unwrap();
    parse(&format!("var x = [{}]", "1,".repeat(2000))).unwrap();
    parse(&format!(
        "if false then {} else {} end",
        "var x=1 ".repeat(1000),
        "var x=2 ".repeat(1000)
    ))
    .unwrap();
}

#[test]
fn invalid_limit_configuration_is_an_error_not_a_panic_or_silent_clamp() {
    for limits in [
        ParseLimits {
            max_nesting: 0,
            ..ParseLimits::default()
        },
        ParseLimits {
            max_nesting: usize::MAX,
            ..ParseLimits::default()
        },
        ParseLimits {
            max_ast_depth: 0,
            ..ParseLimits::default()
        },
        ParseLimits {
            max_ast_depth: usize::MAX,
            ..ParseLimits::default()
        },
    ] {
        let error = parse_with_limits("", limits).unwrap_err();
        assert!(error.message.contains("must be between"));
        assert_eq!(error.limit, None);
    }
    assert_eq!(parse("var =").unwrap_err().limit, None);
    assert_eq!(parse("@").unwrap_err().limit, None);
}
