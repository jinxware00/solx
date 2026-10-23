use solx::analysis::{AnalysisOptions, MAX_NAME_DIAGNOSTICS, SymbolKind, analyze_source};
use solx::check::{check_source, json_analysis_report};
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn names(source: &str) -> Vec<String> {
    let report = analyze_source(
        source,
        AnalysisOptions {
            names: true,
            symbols: false,
        },
    )
    .unwrap();
    report
        .diagnostics
        .iter()
        .map(|diagnostic| {
            assert_eq!(diagnostic.code, "undefined-name");
            source[diagnostic.span.start..diagnostic.span.end].to_owned()
        })
        .collect()
}

fn check(arguments: &[&str], source: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .arg("check")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("non-executing analysis timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn json(arguments: &[&str], source: &[u8], status: i32) -> serde_json::Value {
    let output = check(arguments, source);
    assert_eq!(
        output.status.code(),
        Some(status),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["offsetEncoding"], "utf-8");
    value
}

#[test]
fn captures_later_locals_recursion_parameters_and_shadowing_are_conservative() {
    for source in [
        "var read = fn() return later end const later = 1 print(read())",
        "fn outer(p) var read = fn(q) return later+p+q end var later=1 return read end outer(1)(2)",
        "fn recurse(p) if p then return recurse(false) end return p end recurse(true)",
        "var name = true if name then var name = false print(name) end print(name)",
        "var i=1 for i=i,i do var read=fn() return i end print(read()) end",
        "for key,value in {} do var read=fn() return [key,value] end print(read()) end",
    ] {
        assert!(names(source).is_empty(), "{source}");
    }
    assert_eq!(
        names("fn f(p) var local=1 return p end print(p,local)"),
        ["p", "local"]
    );
}

#[test]
fn globals_are_collected_across_conditional_and_nested_function_bodies() {
    let source = "print(conditional,from_function,from_expression)
        if false then global conditional=true end
        fn create() global from_function=true end
        var callback = [fn() global from_expression=true end]";
    assert!(names(source).is_empty());
    // Predeclaration deliberately does not diagnose use-before-definition.
    assert!(names("print(later) var later=true").is_empty());
}

#[test]
fn branch_loop_and_handler_locals_do_not_leak_into_siblings_or_parents() {
    assert_eq!(
        names(
            "if true then var branch=1 print(branch) elseif condition then var alternate=1 else print(branch,alternate) end print(branch,alternate)"
        ),
        ["condition", "branch", "alternate", "branch", "alternate"]
    );
    assert_eq!(
        names("while condition do var inner=true print(inner) end print(inner)"),
        ["condition", "inner"]
    );
    assert_eq!(
        names(
            "try var protected=1 print(protected) catch error print(error,protected) var recovered=1 end print(error,recovered)"
        ),
        ["protected", "error", "recovered"]
    );
    assert_eq!(
        names(
            "for i=i,limit,step do print(i) end for key,value in source do print(key,value) end print(i,key,value)"
        ),
        ["i", "limit", "step", "source", "i", "key", "value"]
    );
}

#[test]
fn assignments_are_uses_while_properties_keys_annotations_comments_and_strings_are_not() {
    let source = "-- comment with pretend identifiers
        var obj = { missing = 'missing', \"also\" = missing }
        obj.missing = another
        obj[lookup] = value
        missing.member = obj
        var annotated: TypeName = unknown
        var text = 'unknown lookup value'";
    assert_eq!(
        names(source),
        [
            "missing", "another", "lookup", "value", "missing", "unknown"
        ]
    );
    assert_eq!(names("unknown=other"), ["unknown", "other"]);
    assert_eq!(
        names("var obj={unknown=unknown} obj.unknown=unknown"),
        ["unknown", "unknown"]
    );
}

#[test]
fn imports_define_local_bindings_without_loading_modules_and_exports_is_admitted() {
    assert!(names("import './does-not-exist.solx' as absent exports.value = absent.value print(str(num('1')))").is_empty());
    assert!(names("import 'math' print(math.sqrt(4))").is_empty());
    assert_eq!(
        names("if true then import 'math' print(math.pi) end print(math)"),
        ["math"]
    );
    assert_eq!(names("print(math.pi)"), ["math"]);
}

#[test]
fn token_ranges_use_utf8_offsets_and_do_not_select_the_same_word_in_strings_or_keys() {
    let source =
        "var text = '\u{1f600}\u{e9}'\r\nvar obj={missing='missing'} obj.missing=missing(text)";
    let report = analyze_source(
        source,
        AnalysisOptions {
            names: true,
            symbols: false,
        },
    )
    .unwrap();
    assert_eq!(report.diagnostics.len(), 1);
    let diagnostic = &report.diagnostics[0];
    let start = source.rfind("missing(text)").unwrap();
    assert_eq!(
        (diagnostic.span.start, diagnostic.span.end),
        (start, start + 7)
    );
    assert_eq!(diagnostic.span.line, 2);
    let value = json_analysis_report("unicode.solx", &report.diagnostics, None, None);
    assert_eq!(
        value["diagnostics"][0]["range"],
        serde_json::json!({"start":start,"end":start+7})
    );
    assert_eq!(value["diagnostics"][0]["severity"], "error");
}

#[test]
fn symbols_use_full_ast_declarations_and_exact_name_or_import_tokens() {
    let source = "-- fn fake() end
global greeting = '\u{1f600}\u{e9}'
const fixed = true
fn outer(param)
    var local = param
    fn inner() return local end
    import './not-present.solx' as alias
end
import 'math'
var anon = fn(value) const nested=value end";
    let report = analyze_source(
        source,
        AnalysisOptions {
            names: true,
            symbols: true,
        },
    )
    .unwrap();
    assert!(report.diagnostics.is_empty());
    assert_eq!(
        report
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        [
            "greeting", "fixed", "outer", "local", "inner", "alias", "math", "anon", "nested"
        ]
    );
    assert_eq!(
        report
            .symbols
            .iter()
            .map(|symbol| symbol.kind)
            .collect::<Vec<_>>(),
        [
            SymbolKind::Variable,
            SymbolKind::Constant,
            SymbolKind::Function,
            SymbolKind::Variable,
            SymbolKind::Function,
            SymbolKind::Module,
            SymbolKind::Module,
            SymbolKind::Variable,
            SymbolKind::Constant
        ]
    );
    for symbol in &report.symbols {
        assert!(symbol.range.start <= symbol.selection_range.start);
        assert!(symbol.selection_range.end <= symbol.range.end);
        assert!(
            source.is_char_boundary(symbol.range.start)
                && source.is_char_boundary(symbol.range.end)
        );
        let selected = &source[symbol.selection_range.start..symbol.selection_range.end];
        assert_eq!(
            selected,
            if symbol.name == "math" {
                "'math'"
            } else {
                &symbol.name
            }
        );
    }
    let outer = &report.symbols[2];
    let inner = &report.symbols[4];
    assert!(outer.range.start < inner.range.start && outer.range.end > inner.range.end);
    assert_eq!(
        &source[inner.range.start..inner.range.end],
        "fn inner() return local end"
    );
    let value = json_analysis_report("example.solx", &[], None, Some(&report.symbols));
    assert_eq!(value["symbols"][2]["kind"], "function");
    assert_eq!(
        value["symbols"][2]["selectionRange"]["start"],
        outer.selection_range.start
    );
}

#[test]
fn raw_strings_and_complex_expression_terminals_do_not_confuse_name_locations() {
    let source = concat!(
        "var text = \"\"\"fake names {unknown} \\words\r\nvar pretend = missing\"\"\"\n",
        "var obj = { \"\"\"raw key\"\"\" = fn(p) return -p end }\n",
        "var result = [not absent, #other, obj['raw key'](third)^fourth, fifth..text]"
    );
    assert_eq!(
        names(source),
        ["absent", "other", "third", "fourth", "fifth"]
    );
    let report = analyze_source(
        source,
        AnalysisOptions {
            names: false,
            symbols: true,
        },
    )
    .unwrap();
    assert_eq!(
        report
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        ["text", "obj", "result"]
    );
}

#[test]
fn diagnostic_cap_preserves_source_order_and_does_not_prevent_symbol_collection() {
    let source = (0..130)
        .map(|index| format!("unknown{index}\n"))
        .collect::<String>()
        + "var declared=true";
    let report = analyze_source(
        &source,
        AnalysisOptions {
            names: true,
            symbols: true,
        },
    )
    .unwrap();
    assert_eq!(report.diagnostics.len(), MAX_NAME_DIAGNOSTICS);
    assert_eq!(
        &source[report.diagnostics[99].span.start..report.diagnostics[99].span.end],
        "unknown99"
    );
    assert_eq!(report.symbols.last().unwrap().name, "declared");
}

#[test]
fn malformed_input_uses_the_existing_first_syntax_diagnostic_without_partial_analysis() {
    for source in [
        "var okay=unknown var broken=)",
        "var x='unfinished",
        "@",
        "fn f()",
    ] {
        let error = analyze_source(
            source,
            AnalysisOptions {
                names: true,
                symbols: true,
            },
        )
        .unwrap_err();
        assert_eq!(error, check_source(source).unwrap_err());
        let report = json(
            &["--names", "--symbols", "--json", "--stdin"],
            source.as_bytes(),
            1,
        );
        assert_eq!(report["diagnostics"].as_array().unwrap().len(), 1);
        assert_eq!(report["symbols"], serde_json::json!([]));
    }
}

#[test]
fn cli_default_is_unchanged_and_flags_are_independent() {
    let source = b"var defined=unknown";
    assert!(check_source(std::str::from_utf8(source).unwrap()).is_ok());
    let default = json(&["--json", "--stdin"], source, 0);
    assert_eq!(default["diagnostics"], serde_json::json!([]));
    assert!(default.get("symbols").is_none());
    let symbols = json(&["--symbols", "--json", "--stdin"], source, 0);
    assert_eq!(symbols["diagnostics"], serde_json::json!([]));
    assert_eq!(symbols["symbols"][0]["name"], "defined");
    let names = json(&["--names", "--json", "--stdin"], source, 1);
    assert_eq!(names["diagnostics"][0]["code"], "undefined-name");
    assert!(names.get("symbols").is_none());
    assert_eq!(
        check(&["--symbols", "--stdin"], source).status.code(),
        Some(2)
    );
    let human = check(&["--names", "--stdin"], source);
    assert_eq!(human.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&human.stderr).contains("<stdin>:1:13"));
}

#[test]
fn cli_analysis_does_not_execute_calls_loops_imports_or_runtime_failures() {
    let source = b"print('must not print') while true do end import './missing.solx' as missing num('bad') exports.loaded=missing";
    let report = json(&["--names", "--symbols", "--json", "--stdin"], source, 0);
    assert_eq!(report["ok"], true);
    assert_eq!(report["symbols"][0]["name"], "missing");
    let invalid = json(&["--names", "--symbols", "--json", "--stdin"], &[0xff], 2);
    assert_eq!(invalid["error"]["code"], "input-error");
    assert_eq!(invalid["symbols"], serde_json::json!([]));
}
