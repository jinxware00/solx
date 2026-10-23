use solx::interpreter::Interpreter;
use solx::limits::{ExecutionLimits, LimitKind};
use solx::parser::parse;
use solx::runtime::{RuntimeError, error::RuntimeErrorKind};
use std::process::Command;
use std::time::Duration;

fn deadline(error: RuntimeError) {
    assert_eq!(
        error.kind,
        RuntimeErrorKind::LimitExceeded(LimitKind::Deadline),
        "{error}"
    );
    assert!(!error.is_catchable());
    assert!(
        error
            .message
            .contains("execution deadline limit exceeded (0 ms)")
    );
}

#[test]
fn zero_deadline_stops_each_public_entry_before_parsing_resolution_or_effects() {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_duration: Some(Duration::ZERO),
        max_steps: None,
        ..ExecutionLimits::default()
    })
    .unwrap();
    for source in [
        "",
        "@",
        "global forbidden = true",
        "try global forbidden = true catch err global caught = true end",
    ] {
        deadline(interpreter.run_source(source).unwrap_err());
    }
    deadline(
        interpreter
            .run_program(&parse("global forbidden = true").unwrap())
            .unwrap_err(),
    );
    deadline(
        interpreter
            .run_file("a-file-that-need-not-exist.solx")
            .unwrap_err(),
    );
    assert!(interpreter.get_global("forbidden").is_none());
    assert!(interpreter.get_global("caught").is_none());
}

#[test]
fn cancellation_is_sticky_and_reset_does_not_disable_the_deadline() {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_duration: Some(Duration::ZERO),
        ..ExecutionLimits::default()
    })
    .unwrap();
    let token = interpreter.cancellation_token();
    token.cancel();
    for _ in 0..2 {
        let error = interpreter.run_source("").unwrap_err();
        assert_eq!(error.kind, RuntimeErrorKind::Cancelled);
    }
    token.reset();
    deadline(interpreter.run_source("").unwrap_err());
}

#[test]
fn default_and_extremely_large_deadline_budgets_allow_finite_runs() {
    assert_eq!(ExecutionLimits::default().max_duration, None);
    for max_duration in [None, Some(Duration::MAX)] {
        let mut interpreter = Interpreter::with_limits(ExecutionLimits {
            max_duration,
            max_steps: None,
            ..ExecutionLimits::default()
        })
        .unwrap();
        for _ in 0..2 {
            interpreter
                .run_source(
                    "fn work() import 'math' return math.abs(-1) end global okay = work() == 1",
                )
                .unwrap();
            assert_eq!(
                interpreter.get_global("okay"),
                Some(solx::runtime::Value::Boolean(true))
            );
        }
    }
}

#[test]
fn cli_deadline_is_optional_and_independent_of_the_step_budget() {
    let output = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args([
            "--timeout-ms",
            "0",
            "--unlimited-steps",
            "-e",
            "print('must not run')",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("execution deadline limit exceeded (0 ms)")
    );
    for args in [
        vec!["--unlimited-steps", "-e", "print('okay')"],
        vec![
            "--timeout-ms",
            "18446744073709551615",
            "-e",
            "print('okay')",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_solx"))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "okay");
    }
    for value in ["-1", "not-a-number", "18446744073709551616"] {
        let output = Command::new(env!("CARGO_BIN_EXE_solx"))
            .args(["--timeout-ms", value, "-e", "print('bad')"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}
