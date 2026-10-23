use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn cli_runs_loops_with_functions_and_control_flow() {
    let output = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args([
            "--execute",
            r#"
            fn sum()
                var total = 0
                for i = 5, 1, -1 do
                    if i == 4 then continue end
                    if i == 2 then break end
                    total = total + i
                end
                return total
            end
            print(sum())
            var count = 0
            while count < 2 do count = count + 1 print(count) end
            "#,
        ])
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
        "8\n1\n2\n"
    );
}

#[test]
fn repl_accepts_multiline_loops_with_nested_functions_and_keeps_state() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(
        b"var total = 0\nfor i = 1, 3 do\nfn current()\nreturn i\nend\nif i == 2 then\ncontinue\nend\ntotal = total + current()\nend\nwhile total < 6 do\ntotal = total + 1\nend\nprint(total)\nquit\n"
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
    assert!(stdout.contains("> 6\n"), "{stdout}");
}

#[test]
fn cli_reports_loop_control_errors_without_executing_source() {
    let output = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args([
            "--execute",
            "print('unreachable') while true do fn bad() break end end",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("break is only allowed inside a loop")
    );
}
