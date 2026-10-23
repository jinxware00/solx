use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn documented_example_handles_failure_without_stderr_or_failed_exit() {
    let output = Command::new(env!("CARGO_BIN_EXE_solx"))
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/error_handling.solx"
        ))
        .output()
        .unwrap();
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
        "Could not read score: cannot convert 'invalid' to Number\nUsing default score: 0\nProgram continues\n"
    );
}

#[test]
fn uncaught_handler_error_still_has_a_nonzero_exit_and_accurate_position() {
    let output = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args(["-e", "import 'core'\ntry num('bad') catch err\n  core.error('handler failed')\nend\nprint('unreachable')"])
        .output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr)
            .unwrap()
            .replace("\r\n", "\n"),
        "solx: handler failed at <source>:3:3\n"
    );
}

#[test]
fn repl_accepts_multiline_handlers_and_preserves_state_without_retrying_the_body() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(
        b"var attempts = 0\ntry\nattempts = attempts + 1\nnum('bad')\ncatch err\nprint(err.message)\nend\nprint(attempts)\ntry catch 42\nprint(99)\nquit\n"
    ).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(stdout.matches("cannot convert 'bad' to Number").count(), 1);
    assert!(stdout.contains("> 1\n"), "{stdout}");
    assert!(stdout.contains("> 99\n"), "{stdout}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr
            .matches("expected an error name after 'catch'")
            .count(),
        1
    );
}
