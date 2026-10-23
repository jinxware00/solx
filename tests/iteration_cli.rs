use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn cli_prints_list_values_and_sorted_object_pairs() {
    let output = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args(["-e", "for value in [10, nil] do print(value) end for key, value in {z = 3, a = 1} do print(key, value) end"])
        .output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "10\nnil\na\t1\nz\t3\n"
    );
}

#[test]
fn checker_accepts_iteration_without_evaluating_its_source_or_body() {
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
        .write_all(b"for key, value in missing() do while true do end end")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ok"], true);
}

#[test]
fn repl_accepts_multiline_iteration_and_reports_runtime_locations() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"var total = 0\nfor index, value in [10, 20]\ndo\ntotal = total + index + value\nend\nprint(total)\nfor value in nil do end\nprint(42)\nquit\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    assert!(stdout.contains("> 33\n"), "{stdout}");
    assert!(stdout.contains("> 42\n"), "{stdout}");
    assert_eq!(
        String::from_utf8(output.stderr)
            .unwrap()
            .replace("\r\n", "\n"),
        "Error: collection for requires List or Object, got Nil at <source>:1:1\n"
    );
}
