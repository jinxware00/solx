use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn function_example_prints_expected_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_solx"))
        .arg("--execute")
        .arg(include_str!("../examples/functions.solx"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "30\n10\n1\n2\n"
    );
}

#[test]
fn repl_accepts_multiline_functions_and_retains_closures() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(
        b"fn makeCounter()\nvar count = 0\nreturn fn()\nif true then\ncount = count + 1\nreturn count\nend\nend\nend\nvar counter = makeCounter()\nprint(counter())\nprint(counter())\nquit\n"
    ).unwrap();
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
    assert!(stdout.contains("> 1\n> 2\n"), "{stdout}");
}

#[test]
fn print_remains_variadic_including_zero_arguments() {
    let output = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args(["--execute", "print() print(1, true, 'text')"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "\n1\ttrue\ttext\n"
    );
}

#[test]
fn repl_reports_nested_runtime_errors_once_and_keeps_captured_state() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(
        b"fn make() var count = 0 return { fail = fn() count = count + 1 return num('found eof') end, read = fn() return count end } end\nvar state = make()\nfn outer() return state.fail() end\nouter()\nprint(state.read())\nquit\n"
    ).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(stderr, "Error: cannot convert 'found eof' to Number\n");
    let stdout = String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    assert!(stdout.contains("> 1\n"), "{stdout}");
}

#[test]
fn repl_reports_invalid_function_headers_without_consuming_the_next_statement() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"fn invalid(a, a)\nprint(42)\nquit\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(stderr, "Error: duplicate parameter 'a' at 1:15\n");
    let stdout = String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    assert!(stdout.contains("> 42\n"), "{stdout}");
}
