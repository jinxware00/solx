use solx::check::check_source;
use solx::parser::parse;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn check(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .arg("check")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The CLI may reject options before reading; EPIPE is expected in that case.
    let _ = child.stdin.take().unwrap().write_all(input);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("check exceeded ten seconds (must never execute input)");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn json(input: &[u8], expected_code: i32) -> serde_json::Value {
    let output = check(&["--json", "--stdin"], input);
    assert_eq!(
        output.status.code(),
        Some(expected_code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["schemaVersion"], 1);
    assert_eq!(result["offsetEncoding"], "utf-8");
    assert_eq!(result["source"], "<stdin>");
    result
}

#[test]
fn checking_never_runs_statements_calls_or_imports() {
    for source in [
        "",
        "print('must not print')",
        "while true do end",
        "fn f() return f() end f()",
        "import './does-not-exist.solx' as missing",
        "num('not a number')",
        "missing_name()",
    ] {
        let report = json(source.as_bytes(), 0);
        assert_eq!(report["ok"], true);
        assert_eq!(report["diagnostics"], serde_json::json!([]));
        assert_eq!(report["error"], serde_json::Value::Null);
    }
}

#[test]
fn lexer_errors_keep_exact_unicode_byte_ranges_through_the_parser() {
    for (source, bad) in [
        ("var text = '😀é'\r\n@", "@"),
        ("var text = 'ok'\n😀", "😀"),
        ("var x = 'é\\q'", "'é\\q"),
    ] {
        let start = source.find(bad).unwrap();
        let error = check_source(source).unwrap_err();
        assert_eq!(error.code, "lex-error");
        assert_eq!(
            (error.span.start, error.span.end),
            (start, start + bad.len())
        );
        assert_eq!(parse(source).unwrap_err().span, error.span);
        let report = json(source.as_bytes(), 1);
        assert_eq!(
            report["diagnostics"][0]["range"],
            serde_json::json!({"start": start, "end": start + bad.len()})
        );
    }
}

#[test]
fn parser_errors_and_eof_have_precise_ranges_and_only_the_first_failure() {
    for (source, start, end) in [
        ("var x = ) @", 10, 11),
        ("var x = )", 8, 9),
        ("var x =", 7, 7),
    ] {
        // A lexical failure prevents parsing, even if an earlier token would
        // fail parsing. This preserves the real pipeline's existing behavior.
        let report = json(source.as_bytes(), 1);
        assert_eq!(report["diagnostics"].as_array().unwrap().len(), 1);
        assert_eq!(
            report["diagnostics"][0]["range"],
            serde_json::json!({"start": start, "end": end})
        );
        assert_eq!(report["ok"], false);
    }
}

#[test]
fn malformed_strings_numbers_and_nesting_use_real_lexer_parser_errors() {
    for source in ["var x = 'unfinished", "var x = 1e+", "var x = 'a\nb'"] {
        assert_eq!(
            json(source.as_bytes(), 1)["diagnostics"][0]["code"],
            "lex-error"
        );
    }
    let source = format!("{}1{}", "(".repeat(1000), ")".repeat(1000));
    let report = json(source.as_bytes(), 1);
    assert_eq!(report["diagnostics"][0]["code"], "syntax-limit");
}

#[test]
fn json_escapes_messages_and_input_failures_are_not_source_diagnostics() {
    let report = json(b"var x = '\\q'", 1);
    assert!(
        report["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("\\q")
    );
    for source in [vec![0xff], vec![b' '; solx::check::MAX_SOURCE_BYTES + 1]] {
        let report = json(&source, 2);
        assert_eq!(report["ok"], false);
        assert_eq!(report["diagnostics"], serde_json::json!([]));
        assert_eq!(report["error"]["code"], "input-error");
    }
}

#[test]
fn file_checks_support_spaces_and_missing_files_have_structured_errors() {
    let root = std::env::temp_dir().canonicalize().unwrap();
    let path = root.join(format!(
        "solx-check-{}-{} & space.solx",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, "print('not executed')").unwrap();
    let output = check(&["--json", "--", path.to_str().unwrap()], b"");
    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["source"], path.to_str().unwrap());
    assert_eq!(report["ok"], true);
    std::fs::remove_file(&path).unwrap();
    let output = check(&["--json", "--", path.to_str().unwrap()], b"");
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["error"]["code"], "input-error");
}

#[test]
fn human_output_and_invalid_options_are_clear() {
    let output = check(&["--stdin"], b"var x = 1");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("No syntax errors in <stdin>"));
    let output = check(&["--stdin"], b"\n@");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("<stdin>:2:1"));
    for args in [
        vec![],
        vec!["--json"],
        vec!["--stdin", "file.solx"],
        vec!["--stdin", "-e", "print(1)"],
    ] {
        assert_eq!(check(&args, b"").status.code(), Some(2));
    }
}
