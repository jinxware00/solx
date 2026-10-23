use solx::interpreter::Interpreter;
use solx::parser::{ast::Stmt, parse};
use solx::runtime::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn run(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.run_source(source).unwrap();
    interpreter
}

fn field(value: &Value, name: &str) -> Value {
    let Value::Object(fields) = value else {
        panic!("expected Object: {value}")
    };
    fields.borrow().get(name).unwrap().clone()
}

fn entries(value: &Value) -> Vec<Value> {
    let Value::List(items) = value else {
        panic!("expected List: {value}")
    };
    items.borrow().clone()
}

#[test]
fn success_skips_handler_and_preserves_outer_bindings() {
    let interpreter = run("import 'core'
         const err = 'outer'
         var count = 0
         try var local_value = 1 count = count + local_value
         catch err core.error('must not run') end
         core.assert(count == 1)
         core.assert(err == 'outer')");
    assert!(interpreter.get_global("local_value").is_none());
}

#[test]
fn catches_real_runtime_failures_and_continues_after_end() {
    for (body, expected) in [
        ("num('bad')", "cannot convert 'bad' to Number"),
        ("1 + 'x'", "arithmetic requires Number, got String"),
        (
            "core.assert(false, 'bad input')",
            "assertion failed: bad input",
        ),
        ("core.error('explicit')", "explicit"),
        ("fn f(a) end f()", "f() expects 1 argument(s), got 0"),
        ("var x = 1 x()", "cannot call Number; expected Function"),
        (
            "const fixed = 1 fixed = 2",
            "cannot reassign constant 'fixed'",
        ),
        ("var typed: Number = 1 typed = 'x'", "type mismatch"),
        (
            "import 'missing_error_test_module'",
            "unknown built-in module",
        ),
    ] {
        let interpreter = run(&format!(
            "import 'core' try {body} global unreachable = true
             catch err global caught = err end global after = 42"
        ));
        let error = interpreter.get_global("caught").unwrap();
        let Value::String(message) = field(&error, "message") else {
            panic!()
        };
        assert!(message.contains(expected), "{body}: {message}");
        assert!(interpreter.get_global("unreachable").is_none());
        assert_eq!(interpreter.get_global("after"), Some(Value::Number(42.0)));
    }
}

#[test]
fn caught_errors_keep_original_locations_and_innermost_first_call_trace() {
    let interpreter = run(
        "fn inner()\n  num('bad')\nend\nfn outer()\n  inner()\nend\ntry\n  outer()\ncatch err\n  global captured = err\nend",
    );
    let error = interpreter.get_global("captured").unwrap();
    assert_eq!(
        field(&error, "message"),
        Value::String("cannot convert 'bad' to Number".into())
    );
    let location = field(&error, "location");
    assert_eq!(field(&location, "file"), Value::String("<source>".into()));
    assert_eq!(field(&location, "line"), Value::Number(2.0));
    assert_eq!(field(&location, "column"), Value::Number(3.0));
    let trace = entries(&field(&error, "trace"));
    assert_eq!(trace.len(), 2);
    for (frame, name, line) in [
        (&trace[0], "in inner()", 5.0),
        (&trace[1], "in outer()", 8.0),
    ] {
        assert_eq!(field(frame, "description"), Value::String(name.into()));
        assert_eq!(
            field(&field(frame, "location"), "line"),
            Value::Number(line)
        );
    }
}

#[test]
fn failing_handler_runs_once_and_propagates_to_the_outer_handler() {
    run("import 'core'
         const err = 'outer binding'
         var count = 0
         try
             try core.error('original')
             catch err
                 count = count + 1
                 core.assert(err.message == 'original')
                 core.error('handler failed')
             end
         catch err
             core.assert(err.message == 'handler failed')
             count = count + 10
         end
         core.assert(err == 'outer binding')
         core.assert(count == 11)");
}

#[test]
fn try_and_catch_have_separate_scopes_and_completed_mutations_remain() {
    let mut interpreter = run("import 'core'
        var local_name = 'outside'
        var changed = 0
        try
            var local_name = 'try'
            var try_only = 1
            changed = 7
            core.error('failure')
        catch err
            core.assert(local_name == 'outside')
            var local_name = 'catch'
            var catch_only = 2
            core.assert(changed == 7)
        end
        core.assert(local_name == 'outside')
        core.assert(changed == 7)");
    for name in ["try_only", "catch_only", "err"] {
        assert!(
            interpreter
                .run_source(&format!("print({name})"))
                .unwrap_err()
                .message
                .contains("undeclared variable")
        );
    }
}

#[test]
fn closures_keep_try_locals_and_catch_error_snapshots_after_unwinding() {
    run("import 'core'
         var escaped = nil
         fn recover()
             try
                 var saved = 41
                 escaped = fn() return saved end
                 core.error('remember me')
             catch err
                 return fn() return err.message end
             end
         end
         var read_error = recover()
         core.assert(escaped() == 41)
         core.assert(read_error() == 'remember me')
         try core.error('another error') catch err err.message = 'edited' end
         core.assert(read_error() == 'remember me')");
}

#[test]
fn returns_from_either_body_exit_the_function_including_bare_returns() {
    run("import 'core'
         fn success() try return 7 catch err core.error('unexpected') end end
         fn recovered() try return num('bad') catch err return err.message end end
         fn bare() try return catch err return 99 end end
         fn bare_catch() try num('bad') catch err return end end
         core.assert(success() == 7)
         core.assert(recovered() == \"cannot convert 'bad' to Number\")
         core.assert(bare() == nil)
         core.assert(bare_catch() == nil)");
}

#[test]
fn loop_jumps_cross_both_bodies_without_entering_the_handler_on_success() {
    run("import 'core'
         var total = 0
         for i = 1, 5 do
             try
                 if i == 1 then continue end
                 if i == 4 then break end
                 total = total + i
             catch err core.error('unexpected') end
         end
         core.assert(total == 5)
         var i = 0
         var recovered = 0
         while i < 5 do
             i = i + 1
             try core.error('expected')
             catch err
                 if i == 1 then continue end
                 if i == 4 then break end
                 recovered = recovered + i
             end
         end
         core.assert(recovered == 5)
         core.assert(i == 4)");
}

#[test]
fn errors_stop_later_argument_evaluation_without_rolling_back_side_effects() {
    run("import 'core'
         var calls = 0
         fn mark() calls = calls + 1 return calls end
         try print(mark(), num('bad'), mark())
         catch err core.assert(calls == 1) end
         core.assert(calls == 1)");
}

#[test]
fn source_syntax_errors_are_not_caught_and_no_partial_source_executes() {
    for source in [
        "global touched = true try var broken = ) catch err global caught = true end",
        "global touched = true try @ catch err global caught = true end",
        "global touched = true try num('bad') catch err var broken = ) end",
    ] {
        let mut interpreter = Interpreter::new();
        assert!(interpreter.run_source(source).is_err());
        assert!(interpreter.get_global("touched").is_none());
        assert!(interpreter.get_global("caught").is_none());
    }
}

#[test]
fn handler_failure_restores_state_and_uncaught_diagnostics() {
    let mut interpreter = Interpreter::new();
    let error = interpreter.run_source(
        "import 'core'\nvar outer = 10\ntry num('bad')\ncatch err\n  var outer = 99\n  core.error('handler')\nend",
    ).unwrap_err();
    assert_eq!(error.message, "handler");
    let span = error.location.unwrap().span.unwrap();
    assert_eq!((span.line, span.column), (6, 3));
    assert!(error.contexts.is_empty());
    interpreter.run_source("core.assert(outer == 10)").unwrap();
    let next = interpreter.run_source("num('next')").unwrap_err();
    assert_eq!(next.location.unwrap().span.unwrap().line, 1);
    assert!(next.contexts.is_empty());
}

#[test]
fn unlocated_host_ast_uses_nil_location_instead_of_inventing_a_position() {
    let mut program = parse("try num('bad') catch err global caught = err end").unwrap();
    program.statements[0] = program.statements[0].unlocated().clone();
    let Stmt::TryCatch { body, .. } = &mut program.statements[0] else {
        panic!()
    };
    body[0] = body[0].unlocated().clone();
    let mut interpreter = Interpreter::new();
    interpreter.run_program(&program).unwrap();
    let caught = interpreter.get_global("caught").unwrap();
    assert_eq!(field(&caught, "location"), Value::Nil);
    assert!(entries(&field(&caught, "trace")).is_empty());
}

static NEXT_PROJECT: AtomicU64 = AtomicU64::new(0);

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "solx-catch-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, source).unwrap();
        path
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        assert!(
            self.0
                .starts_with(std::env::temp_dir().canonicalize().unwrap())
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn imported_runtime_and_syntax_failures_are_catchable_and_retriable() {
    let project = Project::new();
    let broken = project.write("broken.solx", "var broken = )");
    project.write("state.solx", "exports.value = { attempts = 0 }");
    project.write(
        "failing.solx",
        "import './state.solx' as state\nstate.value.attempts = state.value.attempts + 1\nnum('module error')",
    );
    let entry = project.write(
        "entry.solx",
        "import 'core'
        import './state.solx' as state
        for i = 1, 2 do
            try import './failing.solx'
            catch err
                core.assert(state.value.attempts == i)
                core.assert(err.message == \"cannot convert 'module error' to Number\")
            end
        end
        try import './broken.solx' catch err global syntax_error = err end
        global completed = true",
    );
    let mut interpreter = Interpreter::new();
    interpreter.run_file(&entry).unwrap();
    assert_eq!(
        interpreter.get_global("completed"),
        Some(Value::Boolean(true))
    );
    let error = interpreter.get_global("syntax_error").unwrap();
    let location = field(&error, "location");
    assert_eq!(
        field(&location, "file"),
        Value::String(broken.display().to_string())
    );
    assert_eq!(field(&location, "line"), Value::Number(1.0));
    assert_eq!(field(&location, "column"), Value::Number(14.0));
    let trace = entries(&field(&error, "trace"));
    assert_eq!(trace.len(), 1);
    assert!(
        field(&trace[0], "description")
            .to_string()
            .contains("import './broken.solx'")
    );
    project.write("broken.solx", "exports.answer = 42");
    interpreter
        .run_source("import './broken.solx' as repaired core.assert(repaired.answer == 42)")
        .unwrap();
}

#[test]
fn failed_import_and_function_calls_restore_definition_paths_and_globals() {
    let project = Project::new();
    let module = project.write(
        "function.solx",
        "exports.fail = fn()\n  num('from definition')\nend",
    );
    let entry = project.write(
        "entry.solx",
        "import './function.solx' as m\ntry m.fail() catch err global caught = err end",
    );
    let mut interpreter = Interpreter::new();
    interpreter.run_file(&entry).unwrap();
    let error = interpreter.get_global("caught").unwrap();
    assert_eq!(
        field(&field(&error, "location"), "file"),
        Value::String(module.display().to_string())
    );
    assert_eq!(
        field(&field(&error, "location"), "line"),
        Value::Number(2.0)
    );
    interpreter
        .run_source("try num('interactive') catch err global next = err end")
        .unwrap();
    assert_eq!(
        field(
            &field(&interpreter.get_global("next").unwrap(), "location"),
            "file"
        ),
        Value::String("<source>".into())
    );
}

#[test]
fn host_collection_borrow_errors_are_catchable_without_panicking() {
    let mut interpreter = run("global items = [1]");
    let Value::List(items) = interpreter.get_global("items").unwrap() else {
        panic!()
    };
    {
        let _borrow = items.borrow_mut();
        interpreter
            .run_source("try var size = #items catch err global caught = err end")
            .unwrap();
        assert!(
            field(&interpreter.get_global("caught").unwrap(), "message")
                .to_string()
                .contains("mutably borrowed")
        );
    }
    interpreter.run_source("global size = #items").unwrap();
    assert_eq!(interpreter.get_global("size"), Some(Value::Number(1.0)));
}
