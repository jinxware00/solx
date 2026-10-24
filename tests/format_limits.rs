use solx::interpreter::Interpreter;
use solx::limits::{ExecutionLimits, LimitKind};
use solx::runtime::{RuntimeError, Value, error::RuntimeErrorKind};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn limited(bytes: usize, work: u64) -> Interpreter {
    Interpreter::with_limits(ExecutionLimits {
        max_format_bytes: bytes,
        max_format_work: work,
        ..ExecutionLimits::default()
    })
    .unwrap()
}

fn limit(error: RuntimeError, expected: LimitKind) {
    assert_eq!(
        error.kind,
        RuntimeErrorKind::LimitExceeded(expected),
        "{error}"
    );
    assert!(!error.is_catchable());
}

fn cli(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if started.elapsed() > Duration::from_secs(20) {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "format test timed out: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn utf8_size_boundary_is_shared_by_conversion_and_concatenation() {
    for expression in ["str('é😀')", "'é' .. '😀'"] {
        let source = format!("global result = {expression}");
        let mut interpreter = limited(6, 1000);
        interpreter.run_source(&source).unwrap();
        assert_eq!(
            interpreter.get_global("result"),
            Some(Value::String("é😀".into()))
        );
        limit(
            limited(5, 1000).run_source(&source).unwrap_err(),
            LimitKind::FormatBytes,
        );
    }
    let mut interpreter = limited(0, 1000);
    interpreter
        .run_source("global empty = str('') .. ''")
        .unwrap();
    assert_eq!(
        interpreter.get_global("empty"),
        Some(Value::String(String::new()))
    );
    limit(
        interpreter.run_source("print()").unwrap_err(),
        LimitKind::FormatBytes,
    );
}

#[test]
fn complete_print_line_includes_tabs_and_newline_and_fails_before_output() {
    let source = "print('é', 'x')";
    let good = cli(&["--max-format-bytes", "5", "-e", source]);
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    assert_eq!(good.stdout, "é\tx\n".as_bytes());
    let bad = cli(&["--max-format-bytes", "4", "-e", source]);
    assert_eq!(bad.status.code(), Some(1));
    assert!(bad.stdout.is_empty());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("formatted byte limit exceeded (4)"));
    let empty = cli(&["--max-format-bytes", "1", "-e", "print()"]);
    assert!(empty.status.success());
    assert_eq!(empty.stdout, b"\n");
}

#[test]
fn work_budget_is_shared_by_calls_and_resets_for_the_next_public_run() {
    let mut interpreter = limited(100, 5);
    let error = interpreter
        .run_source(
            "fn convert() return str('a') end global first = convert() global second = convert()",
        )
        .unwrap_err();
    limit(error, LimitKind::FormatWork);
    assert_eq!(
        interpreter.get_global("first"),
        Some(Value::String("a".into()))
    );
    assert!(interpreter.get_global("second").is_none());
    interpreter
        .run_source("global recovered = convert()")
        .unwrap();
    assert_eq!(
        interpreter.get_global("recovered"),
        Some(Value::String("a".into()))
    );
    let mut zero = limited(100, 0);
    zero.run_source("global arithmetic = 6 * 7").unwrap();
    limit(
        zero.run_source("str('')").unwrap_err(),
        LimitKind::FormatWork,
    );
}

#[test]
fn format_exhaustion_bypasses_catch_but_retains_completed_effects() {
    for (bytes, work, expected) in [
        (2, 1000, LimitKind::FormatBytes),
        (100, 1, LimitKind::FormatWork),
    ] {
        let mut interpreter = limited(bytes, work);
        let error = interpreter.run_source(
            "global before = 42 try global forbidden = str([1,2,3]) catch e global caught = true end"
        ).unwrap_err();
        limit(error, expected);
        assert_eq!(interpreter.get_global("before"), Some(Value::Integer(42)));
        assert!(interpreter.get_global("forbidden").is_none());
        assert!(interpreter.get_global("caught").is_none());
        interpreter.run_source("global after = 7").unwrap();
    }
}

#[test]
fn cycles_shared_values_and_opaque_scalars_keep_their_language_text() {
    let mut interpreter = limited(1000, 20_000);
    interpreter
        .run_source(
            r#"
        import 'list'
        var cycle = []
        list.append(cycle, cycle)
        var shared = [nil, 'é']
        fn named() end
        global text = str([cycle, shared, shared, named, print, list])
    "#,
        )
        .unwrap();
    assert_eq!(
        interpreter.get_global("text"),
        Some(Value::String(
            "[[<cycle>], [nil, é], [nil, é], <function named>, <function print>, <module list>]"
                .into()
        ))
    );
}

#[test]
fn host_borrow_conflicts_are_catchable_and_release_recovers_conversion() {
    let mut interpreter = limited(100, 1000);
    interpreter
        .run_source("global items = [1] global alias = {items = items}")
        .unwrap();
    let value = interpreter.get_global("items").unwrap();
    let Value::List(items) = &value else {
        unreachable!()
    };
    let guard = items.borrow_mut();
    interpreter
        .run_source("try str(alias) catch e global caught = e.message end")
        .unwrap();
    assert_eq!(
        interpreter.get_global("caught"),
        Some(Value::String(
            "formatting: collection is mutably borrowed".into()
        ))
    );
    assert_eq!(value.to_solx_string(), "<borrowed>");
    drop(guard);
    interpreter
        .run_source("global recovered = str(alias)")
        .unwrap();
    assert_eq!(
        interpreter.get_global("recovered"),
        Some(Value::String("{items = [1]}".into()))
    );
}

#[test]
fn deep_chains_format_linearly_in_a_watchdog_child() {
    // Modest heaps, bounded output and no concurrent children. A future stack
    // regression fails this child rather than aborting the Rust test runner.
    let output = cli(&[
        "--max-format-bytes",
        "50000",
        "--max-format-work",
        "200000",
        "-e",
        r#"
        import 'string'
        var chain = nil
        for i=1,4000 do chain = [chain] end
        print(string.byte_len(str(chain)))
        chain = nil
        for i=1,4000 do chain = {child = chain} end
        print(string.byte_len(str(chain)))
    "#,
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"8003\n40003\n");
}

#[test]
fn compact_shared_graph_cannot_expand_past_the_format_limit() {
    let output = cli(&[
        "--unlimited-steps",
        "--max-format-bytes",
        "1024",
        "-e",
        "var x = [0] for i=1,30 do x=[x,x] end print(x)",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("formatted byte limit exceeded (1024)")
    );
    let output = cli(&[
        "--unlimited-steps",
        "--max-format-work",
        "2",
        "-e",
        "print('hello')",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("formatting work limit exceeded (2)"));
}

#[test]
fn imports_share_format_work_and_a_failed_module_can_be_retried() {
    let base = std::env::temp_dir().canonicalize().unwrap();
    let root = base.join(format!(
        "solx-format-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf, std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let path = self.0.canonicalize().unwrap();
            assert_eq!(path.parent(), Some(self.1.as_path()));
            assert!(
                path.file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("solx-format-")
            );
            std::fs::remove_dir_all(path).unwrap();
        }
    }
    let _cleanup = Cleanup(root.clone(), base);
    let entry = root.join("entry.solx");
    std::fs::write(root.join("child.solx"), "exports.text = str('a')").unwrap();
    std::fs::write(&entry, "global prefix = str('a') import './child.solx'").unwrap();
    let mut interpreter = limited(100, 5);
    limit(
        interpreter.run_file(&entry).unwrap_err(),
        LimitKind::FormatWork,
    );
    std::fs::write(
        &entry,
        "import './child.solx' global recovered = child.text",
    )
    .unwrap();
    interpreter.run_file(&entry).unwrap();
    assert_eq!(
        interpreter.get_global("recovered"),
        Some(Value::String("a".into()))
    );
}
