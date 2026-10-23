//! Adversarial inputs run in real child processes: a stack abort or hang is a
//! failed test, and the watchdog terminates only that test's child.
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn run(args: &[&str], input: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    } else {
        drop(child.stdin.take());
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "child timed out: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn cli_stops_runaway_loops_with_a_source_position_and_nonzero_exit() {
    for source in ["\n while true do end", "\n for i=1,1e100 do end"] {
        let output = run(&["--max-steps", "100", "-e", source], None);
        assert_eq!(output.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("execution step limit exceeded (100) at <source>:2:2"),
            "{stderr}"
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn default_depth_guards_stop_adversarial_sources_without_stack_aborts() {
    for (source, message) in [
        (
            format!("{}1{}", "(".repeat(1000), ")".repeat(1000)),
            "parser nesting",
        ),
        (format!("{}true", "not ".repeat(1000)), "parser nesting"),
        (format!("{}1", "1^".repeat(1000)), "parser nesting"),
        (
            format!("{}1{}", "[".repeat(1000), "]".repeat(1000)),
            "parser nesting",
        ),
        (
            format!("{}{}", "if true then ".repeat(1000), "end ".repeat(1000)),
            "parser nesting",
        ),
        (
            format!("{}{}", "fn f() ".repeat(1000), "end ".repeat(1000)),
            "parser nesting",
        ),
        (format!("x{}", ".x".repeat(1000)), "AST depth"),
        (format!("1{}", "+1".repeat(1000)), "AST depth"),
        (format!("1{}", "+1".repeat(120)), "evaluation depth"),
        ("fn f() return f() end f()".into(), "depth limit"),
        (
            "fn a() return b() end fn b() return a() end a()".into(),
            "depth limit",
        ),
        (
            format!(
                "fn f() {}f(){} end f()",
                "if true then ".repeat(20),
                " end".repeat(20)
            ),
            "evaluation depth",
        ),
    ] {
        let output = run(&["-e", &source], None);
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(message), "{message}: {stderr}");
        assert!(!stderr.contains("stack overflow"));
    }
}

#[test]
fn combined_import_parse_clone_and_evaluation_depths_fail_cleanly() {
    let base = std::env::temp_dir().canonicalize().unwrap();
    let root = base.join(format!(
        "solx-cli-limits-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    for index in 0..40 {
        std::fs::write(
            root.join(format!("m{index}.solx")),
            format!("import './m{}.solx'", index + 1),
        )
        .unwrap();
    }
    std::fs::write(root.join("m40.solx"), "").unwrap();
    let entry = root.join("m0.solx");
    let output = run(&[entry.to_str().unwrap()], None);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("import depth limit exceeded (16)"));
    for leaf in [
        format!("{}1{}", "(".repeat(1000), ")".repeat(1000)),
        format!(
            "fn f() {}f(){} end f()",
            "if true then ".repeat(20),
            " end".repeat(20)
        ),
        format!("fn f() return 1{} end f()", "+1".repeat(110)),
        "fn f() return f() end f()".into(),
    ] {
        std::fs::write(root.join("m15.solx"), leaf).unwrap();
        let output = run(&[entry.to_str().unwrap()], None);
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("limit exceeded"));
    }
    let checked = root.canonicalize().unwrap();
    assert_eq!(checked.parent(), Some(base.as_path()));
    assert!(
        checked
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("solx-cli-limits-")
    );
    std::fs::remove_dir_all(checked).unwrap();
}

#[test]
fn cli_validates_options_and_supports_explicit_unlimited_steps() {
    let output = run(
        &["--unlimited-steps", "-e", "for i=1,5 do print(i) end"],
        None,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        "1\n2\n3\n4\n5\n"
    );
    for args in [
        vec!["--max-steps", "-1"],
        vec!["--max-steps", "abc"],
        vec!["--max-steps", "1", "--unlimited-steps"],
        vec!["--max-call-depth", "0"],
        vec!["--max-call-depth", "33"],
        vec!["--max-import-depth", "0"],
        vec!["--max-import-depth", "17"],
    ] {
        let output = run(&args, None);
        assert!(!output.status.success(), "{args:?}");
        assert!(!output.stderr.is_empty());
    }
    let output = run(
        &["--max-call-depth", "2", "-e", "fn f() return f() end f()"],
        None,
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("function call depth limit exceeded (2)")
    );
}

#[test]
fn repl_recovers_from_budget_and_parser_limits_without_replaying_side_effects() {
    let input = format!(
        "var count=0\ncount=count+1 while true do end\nprint(count)\n{}1{}\nprint(42)\nquit\n",
        "(".repeat(100),
        ")".repeat(100)
    );
    let output = run(&["--max-steps", "100"], Some(&input));
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(stdout.contains("> 1\n"), "{stdout}");
    assert!(stdout.contains("> 42\n"), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("execution step limit exceeded").count(), 1);
    assert_eq!(stderr.matches("parser nesting limit exceeded").count(), 1);
}

#[test]
fn execute_and_repl_runs_get_fresh_budgets() {
    let output = run(
        &["--max-steps", "4", "-e", "print(1)", "-i"],
        Some("print(2)\nquit\n"),
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(stdout.starts_with("1\n"));
    assert!(stdout.contains("> 2\n"));
}
