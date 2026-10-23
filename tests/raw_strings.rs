use solx::check::check_source;
use solx::interpreter::Interpreter;
use solx::lexer::{TokenKind, tokenize};
use solx::runtime::Value;
use std::io::Write;
use std::process::{Command, Stdio};

const DELIMITER: &str = "\"\"\"";

fn raw(body: &str) -> String {
    format!("{DELIMITER}{body}{DELIMITER}")
}

#[test]
fn raw_content_preserves_unicode_whitespace_line_endings_and_backslashes() {
    for body in [
        "",
        "one line",
        "\n  first\n    second\n",
        "\r\n  first\r\nsecond\r\n",
        "\rstandalone carriage return\r",
        "é🙂\t\0末尾",
        r"C:\Users\name\file.txt \n \t \q \u{1234}",
        "{missing()} {{braces}} -- comment\nend @ 'single' \"one\" \"\"two\"\" text",
    ] {
        let source = raw(body);
        let tokens = tokenize(&source).unwrap();
        assert_eq!(tokens.len(), 2, "{source:?}");
        assert_eq!(tokens[0].kind, TokenKind::String(body.into()));
        assert_eq!(
            (tokens[0].span.start, tokens[0].span.end),
            (0, source.len())
        );
        assert_eq!(tokens[1].kind, TokenKind::Eof);
        assert_eq!(tokens[1].span.start, source.len());
    }
}

#[test]
fn raw_delimiters_use_the_first_three_quotes_even_after_a_backslash() {
    let source = format!("{} + 1", raw("ends in a backslash\\"));
    let tokens = tokenize(&source).unwrap();
    assert_eq!(
        tokens[0].kind,
        TokenKind::String("ends in a backslash\\".into())
    );
    assert_eq!(tokens[1].kind, TokenKind::Plus);
    assert_eq!(tokens[2].kind, TokenKind::Number("1".into()));
    assert_eq!(
        tokenize(&"\"".repeat(6)).unwrap()[0].kind,
        TokenKind::String(String::new())
    );
    assert_eq!(tokenize(&"\"".repeat(12)).unwrap().len(), 3);
    let error = tokenize(&"\"".repeat(7)).unwrap_err();
    assert_eq!(error.message, "unterminated string literal");
    assert_eq!((error.span.start, error.span.end), (6, 7));
}

#[test]
fn multiline_raw_tokens_preserve_following_unicode_byte_and_line_positions() {
    let source = format!("var text = {} next", raw("\r\n  é🙂\n"));
    let tokens = tokenize(&source).unwrap();
    let string = &tokens[3];
    assert_eq!((string.span.line, string.span.column), (1, 12));
    assert_eq!(string.span.start, 11);
    assert_eq!(string.span.end, source.find(" next").unwrap());
    let next = &tokens[4];
    assert_eq!(next.kind, TokenKind::Identifier("next".into()));
    assert_eq!((next.span.line, next.span.column), (3, 5));
    assert_eq!(next.span.start, source.find("next").unwrap());
}

#[test]
fn unterminated_raw_literals_report_the_opening_delimiter_through_eof() {
    for body in ["", "\"", "\"\"", "\nunfinished é🙂\r\n\\"] {
        let source = format!("-- prefix\n  var text = {DELIMITER}{body}");
        let error = tokenize(&source).unwrap_err();
        assert_eq!(error.message, "unterminated raw string literal");
        assert_eq!((error.line, error.column), (2, 14));
        assert_eq!(
            (error.span.start, error.span.end),
            (source.find(DELIMITER).unwrap(), source.len())
        );
        let diagnostic = check_source(&source).unwrap_err();
        assert_eq!(diagnostic.code, "lex-error");
        assert_eq!(diagnostic.span, error.span);
    }
}

#[test]
fn ordinary_strings_keep_existing_escapes_and_newline_errors() {
    assert_eq!(
        tokenize(r#""line\n\t\"\\""#).unwrap()[0].kind,
        TokenKind::String("line\n\t\"\\".into())
    );
    assert_eq!(tokenize("''''''").unwrap().len(), 4);
    assert_eq!(
        tokenize("'line\nbreak'").unwrap_err().message,
        "newline in string literal"
    );
    assert_eq!(
        tokenize("\"line\nbreak\"").unwrap_err().message,
        "newline in string literal"
    );
    assert_eq!(
        tokenize(r#""\q""#).unwrap_err().message,
        "unsupported escape sequence '\\q'"
    );
    assert_eq!(
        tokenize("\"unfinished").unwrap_err().message,
        "unterminated string literal"
    );
}

#[test]
fn raw_strings_are_ordinary_string_values_without_interpolation_or_code_execution() {
    let body = "\n{missing()}\nend -- text\nC:\\new\\file\n";
    let source = format!(
        "import 'core' import 'string' global text = {} global length = #text core.assert(core.type(text) == 'String') global copy = text .. {}",
        raw(body),
        raw("suffix")
    );
    let mut interpreter = Interpreter::new();
    interpreter.run_source(&source).unwrap();
    assert_eq!(
        interpreter.get_global("text"),
        Some(Value::String(body.into()))
    );
    assert_eq!(
        interpreter.get_global("length"),
        Some(Value::Integer(body.chars().count() as i64))
    );
    assert_eq!(
        interpreter.get_global("copy"),
        Some(Value::String(format!("{body}suffix")))
    );
}

#[test]
fn triple_quotes_can_be_included_by_concatenating_an_ordinary_escaped_string() {
    let source = format!(
        "global text = {} .. \"\\\"\\\"\\\"\" .. {}",
        raw("before"),
        raw("after")
    );
    let mut interpreter = Interpreter::new();
    interpreter.run_source(&source).unwrap();
    assert_eq!(
        interpreter.get_global("text"),
        Some(Value::String("before\"\"\"after".into()))
    );
}

#[test]
fn cli_and_json_checker_accept_complete_raw_literals_and_report_incomplete_ranges() {
    let source = format!("print({})", raw("first\nsecond"));
    let output = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args(["-e", &source])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "first\nsecond\n"
    );
    for (source, success) in [
        (format!("var text = {}", raw("\n@ ignored\n")), true),
        (format!("var text = {DELIMITER}\nunfinished"), false),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
            .args(["check", "--json", "--stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.success(), success);
        assert!(output.stderr.is_empty());
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["ok"], success);
        if !success {
            assert_eq!(
                report["diagnostics"][0]["range"],
                serde_json::json!({"start": 11, "end": source.len()})
            );
            assert_eq!(
                report["diagnostics"][0]["message"],
                "unterminated raw string literal"
            );
        }
    }
}

#[test]
fn repl_collects_a_raw_literal_until_the_closing_delimiter_and_keeps_following_input() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = format!(
        "var text = {DELIMITER}\nfirst\nsecond\n{DELIMITER}\nprint(text == '\\nfirst\\nsecond\\n')\nprint(42)\nquit\n"
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    assert!(stdout.contains("> true\n"), "{stdout}");
    assert!(stdout.contains("> 42\n"), "{stdout}");
}

#[test]
fn repl_reports_unfinished_raw_input_at_eof_without_executing_its_prefix() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = format!("print('unreachable') var text = {DELIMITER}\nunfinished\n");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("unreachable"));
    assert_eq!(
        String::from_utf8(output.stderr)
            .unwrap()
            .replace("\r\n", "\n"),
        "Error: incomplete input at end of input\n"
    );
}

#[test]
fn repl_applies_the_source_budget_to_the_entire_raw_literal_and_recovers() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args(["--max-source-bytes", "32"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = format!("var text = {DELIMITER}\n01234567890123456789\nprint(42)\nquit\n");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stderr)
            .unwrap()
            .replace("\r\n", "\n"),
        "Error: source byte limit exceeded (32)\n"
    );
    let stdout = String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    assert!(stdout.contains("> 42\n"), "{stdout}");
}
