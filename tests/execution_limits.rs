use solx::interpreter::Interpreter;
use solx::limits::{ExecutionLimits, LimitKind, ParseLimits};
use solx::parser::{
    ast::{Expr, Program, Stmt},
    parse,
};
use solx::runtime::{RuntimeError, Value, error::RuntimeErrorKind};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

fn limited(steps: u64) -> Interpreter {
    Interpreter::with_limits(ExecutionLimits {
        max_steps: Some(steps),
        ..ExecutionLimits::default()
    })
    .unwrap()
}

fn kind(error: RuntimeError, expected: LimitKind) {
    assert_eq!(
        error.kind,
        RuntimeErrorKind::LimitExceeded(expected),
        "{error}"
    );
    assert!(!error.is_catchable());
}

#[test]
fn steps_have_an_exact_boundary_and_reset_only_on_public_runs() {
    let mut interpreter = limited(2);
    for n in 1..=4 {
        interpreter
            .run_source(&format!("global value = {n}"))
            .unwrap();
        assert_eq!(
            interpreter.get_global("value"),
            Some(Value::Number(n as f64))
        );
    }
    let program = parse("global value = 9").unwrap();
    interpreter.run_program(&program).unwrap();
    interpreter.run_program(&program).unwrap();
    let mut denied = limited(1);
    kind(
        denied.run_source("global forbidden = 1").unwrap_err(),
        LimitKind::Steps,
    );
    assert!(denied.get_global("forbidden").is_none());
    let mut zero = limited(0);
    zero.run_source("").unwrap();
    kind(zero.run_source("1").unwrap_err(), LimitKind::Steps);
    // The unused operand is neither evaluated nor charged.
    limited(3)
        .run_source("global value = false and missing")
        .unwrap();
}

#[test]
fn empty_loops_continue_and_caught_errors_cannot_evade_the_budget() {
    for source in [
        "while true do end",
        "for i=1,1e100 do end",
        "while true do continue end",
        "for i=1,1e100 do continue end",
        "import 'core' while true do try core.error('again') catch e end end",
        "try while true do end catch e global caught = true end",
        "try for i=1,1e100 do end catch e global caught = true end",
    ] {
        let mut interpreter = limited(100);
        let error = interpreter.run_source(source).unwrap_err();
        assert!(error.location.as_ref().unwrap().span.is_some());
        kind(error, LimitKind::Steps);
        assert!(interpreter.get_global("caught").is_none());
        interpreter.run_source("global recovered = 42").unwrap();
    }
}

#[test]
fn interruption_restores_scopes_and_keeps_completed_effects_and_closures() {
    let mut interpreter = limited(200);
    interpreter.run_source("global outer = 5").unwrap();
    kind(
        interpreter
            .run_source(
                "fn work() var outer = 9 global changed = 7
         global saved = fn() return outer end
         try while true do var local_only = 1 end
         catch err global caught = true end end work()",
            )
            .unwrap_err(),
        LimitKind::Steps,
    );
    assert_eq!(interpreter.get_global("outer"), Some(Value::Number(5.0)));
    assert_eq!(interpreter.get_global("changed"), Some(Value::Number(7.0)));
    assert!(interpreter.get_global("caught").is_none());
    assert!(interpreter.get_global("local_only").is_none());
    interpreter.run_source("global captured = saved()").unwrap();
    assert_eq!(interpreter.get_global("captured"), Some(Value::Number(9.0)));
    let error = interpreter.run_source("\n  num('bad')").unwrap_err();
    assert!(error.is_catchable());
    assert_eq!(error.location.unwrap().span.unwrap().line, 2);
    assert!(error.contexts.is_empty());
}

#[test]
fn call_limits_report_context_and_unwind_even_through_handlers() {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_call_depth: 3,
        ..ExecutionLimits::default()
    })
    .unwrap();
    interpreter
        .run_source("fn f(n)\n if n > 0 then return f(n-1) end\n return 7\nend")
        .unwrap();
    interpreter.run_source("global answer = f(2)").unwrap();
    let error = interpreter
        .run_source("try f(3) catch e global caught = true end")
        .unwrap_err();
    assert_eq!(error.contexts.len(), 4);
    assert_eq!(error.location.as_ref().unwrap().span.unwrap().line, 2);
    kind(error, LimitKind::CallDepth);
    assert!(interpreter.get_global("caught").is_none());
    interpreter.run_source("global answer = f(2)").unwrap();
    assert_eq!(interpreter.get_global("answer"), Some(Value::Number(7.0)));
}

#[test]
fn combined_depth_covers_expression_and_function_recursion() {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_evaluation_depth: 12,
        ..ExecutionLimits::default()
    })
    .unwrap();
    for source in [
        "fn f() return f() end f()".to_owned(),
        format!("global value = 1{}", "+1".repeat(20)),
    ] {
        kind(
            interpreter.run_source(&source).unwrap_err(),
            LimitKind::EvaluationDepth,
        );
        interpreter.run_source("global after = 1").unwrap();
    }
}

#[test]
fn host_ast_is_checked_before_any_execution_or_function_clone() {
    let mut expression = Expr::Nil;
    for _ in 0..200 {
        expression = Expr::Property {
            object: Box::new(expression),
            name: "x".into(),
        };
    }
    let mut program = parse("global should_not_run = 1").unwrap();
    program.statements.push(Stmt::Expression(expression));
    let mut interpreter = Interpreter::new();
    kind(
        interpreter.run_program(&program).unwrap_err(),
        LimitKind::AstDepth,
    );
    assert!(interpreter.get_global("should_not_run").is_none());
    interpreter
        .run_program(&Program { statements: vec![] })
        .unwrap();
}

#[test]
fn cancellation_is_sticky_and_host_controlled_with_explicit_recovery() {
    let mut interpreter = Interpreter::new();
    let token = interpreter.cancellation_token();
    token.cancel();
    for _ in 0..2 {
        let error = interpreter.run_source("global forbidden = 1").unwrap_err();
        assert_eq!(error.kind, RuntimeErrorKind::Cancelled);
        assert!(!error.is_catchable());
    }
    assert!(interpreter.get_global("forbidden").is_none());
    token.reset();
    interpreter.run_source("global recovered = 7").unwrap();
}

#[test]
fn another_thread_can_cancel_running_code_and_the_interpreter_remains_usable() {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_steps: None,
        ..ExecutionLimits::default()
    })
    .unwrap();
    interpreter.run_source("global outer = 7").unwrap();
    let token = interpreter.cancellation_token();
    let controller = token.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        controller.cancel();
    });
    let error = interpreter.run_source("fn work() var outer = 0 try while true do end catch e global caught = true end end work()").unwrap_err();
    thread.join().unwrap();
    assert_eq!(error.kind, RuntimeErrorKind::Cancelled);
    assert!(!error.is_catchable());
    assert!(interpreter.get_global("caught").is_none());
    assert_eq!(interpreter.get_global("outer"), Some(Value::Number(7.0)));
    token.reset();
    interpreter.run_source("global answer = outer + 1").unwrap();
    assert_eq!(interpreter.get_global("answer"), Some(Value::Number(8.0)));
}

struct Project {
    root: PathBuf,
}
impl Project {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "solx-limits-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        Self { root }
    }
    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, source).unwrap();
        path
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let path = self.root.canonicalize().unwrap();
        assert_eq!(
            path.parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("solx-limits-")
        );
        fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn imports_share_steps_and_failed_modules_can_be_retried() {
    let project = Project::new();
    let entry = project.write(
        "entry.solx",
        "import './child.solx' as child global answer = child.answer",
    );
    project.write("child.solx", "var x=1 var y=2 var z=3 exports.answer=7");
    let mut interpreter = limited(8);
    kind(interpreter.run_file(&entry).unwrap_err(), LimitKind::Steps);
    assert!(interpreter.get_global("answer").is_none());
    project.write("child.solx", "exports.answer=9");
    interpreter.run_file(&entry).unwrap();
    assert_eq!(interpreter.get_global("answer"), Some(Value::Number(9.0)));
}

#[test]
fn successful_module_cache_survives_a_later_entry_budget_failure() {
    let project = Project::new();
    let entry = project.write(
        "entry.solx",
        "import './child.solx' as child global answer = child.answer",
    );
    project.write("child.solx", "var x=1 var y=2 var z=3 exports.answer=7");
    let mut interpreter = limited(12);
    kind(interpreter.run_file(&entry).unwrap_err(), LimitKind::Steps);
    project.write("child.solx", "exports.answer=9");
    interpreter.run_file(&entry).unwrap();
    assert_eq!(interpreter.get_global("answer"), Some(Value::Number(7.0)));
}

#[test]
fn import_depth_excludes_entry_and_cached_modules_and_restores_loading_state() {
    let project = Project::new();
    let entry = project.write("entry.solx", "import './a.solx'");
    project.write("a.solx", "import './b.solx'");
    project.write("b.solx", "import './c.solx'");
    project.write("c.solx", "exports.answer = 7");
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_import_depth: 2,
        ..ExecutionLimits::default()
    })
    .unwrap();
    let error = interpreter.run_file(&entry).unwrap_err();
    assert_eq!(error.contexts.len(), 3);
    assert_eq!(
        error.location.as_ref().unwrap().file,
        Some(project.root.join("b.solx").canonicalize().unwrap())
    );
    kind(error, LimitKind::ImportDepth);
    project.write("b.solx", "exports.answer = 7");
    interpreter.run_file(&entry).unwrap();
    // b is cached, so c -> d -> cached b has only two active imports.
    project.write("c.solx", "import './d.solx'");
    project.write("d.solx", "import './b.solx'");
    interpreter.run_source("import './c.solx'").unwrap();
}

#[test]
fn parser_limits_inside_imports_bypass_catch_and_allow_later_retry() {
    let project = Project::new();
    let entry = project.write(
        "entry.solx",
        "try import './child.solx' catch e global caught = true end",
    );
    project.write(
        "child.solx",
        &format!("{}1{}", "(".repeat(200), ")".repeat(200)),
    );
    let mut interpreter = Interpreter::new();
    kind(
        interpreter.run_file(&entry).unwrap_err(),
        LimitKind::ParseNesting,
    );
    assert!(interpreter.get_global("caught").is_none());
    project.write("child.solx", "exports.answer = 7");
    interpreter.run_file(&entry).unwrap();
    interpreter
        .run_source("import './child.solx' global answer = child.answer")
        .unwrap();
    assert_eq!(interpreter.get_global("answer"), Some(Value::Number(7.0)));
}

#[test]
fn cancelled_imports_restore_file_context_and_remain_retryable() {
    let project = Project::new();
    let entry = project.write("entry.solx", "import './child.solx'");
    project.write("child.solx", "while true do end");
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_steps: None,
        ..ExecutionLimits::default()
    })
    .unwrap();
    let token = interpreter.cancellation_token();
    let controller = token.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        controller.cancel();
    });
    let error = interpreter.run_file(&entry).unwrap_err();
    thread.join().unwrap();
    assert_eq!(error.kind, RuntimeErrorKind::Cancelled);
    token.reset();
    project.write("child.solx", "exports.answer = 7");
    interpreter.run_file(&entry).unwrap();
    interpreter
        .run_source("global answer = child.answer")
        .unwrap();
    assert_eq!(interpreter.get_global("answer"), Some(Value::Number(7.0)));
}

#[test]
fn invalid_host_limits_are_rejected() {
    for limits in [
        ExecutionLimits {
            max_call_depth: 0,
            ..ExecutionLimits::default()
        },
        ExecutionLimits {
            max_call_depth: usize::MAX,
            ..ExecutionLimits::default()
        },
        ExecutionLimits {
            max_import_depth: 0,
            ..ExecutionLimits::default()
        },
        ExecutionLimits {
            max_import_depth: usize::MAX,
            ..ExecutionLimits::default()
        },
        ExecutionLimits {
            max_evaluation_depth: 0,
            ..ExecutionLimits::default()
        },
        ExecutionLimits {
            max_evaluation_depth: usize::MAX,
            ..ExecutionLimits::default()
        },
        ExecutionLimits {
            parse: ParseLimits {
                max_ast_depth: 0,
                ..ParseLimits::default()
            },
            ..ExecutionLimits::default()
        },
    ] {
        assert!(
            Interpreter::with_limits(limits)
                .err()
                .unwrap()
                .message
                .contains("must be between")
        );
    }
}
