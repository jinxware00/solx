use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use solx::interpreter::Interpreter;
use solx::runtime::Value;

static NEXT_PROJECT: AtomicU64 = AtomicU64::new(0);

struct Project {
    root: PathBuf,
    temp_root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let temp_root = std::env::temp_dir().canonicalize().unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = temp_root.join(format!(
            "solx-modules-{}-{nonce}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self { root, temp_root }
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.root.join(name);
        assert!(path.starts_with(&self.root));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, source).unwrap();
        path
    }

    fn interpreter(&self) -> Interpreter {
        let entry = self.write("entry.solx", "");
        let mut interpreter = Interpreter::new();
        interpreter.run_file(entry).unwrap();
        interpreter
    }

    fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_solx"));
        command.current_dir(&self.root);
        command
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        // Restrict recursive cleanup to this test's unique canonical temp directory.
        assert!(
            self.root.is_absolute()
                && self.root.starts_with(&self.temp_root)
                && self.root != self.temp_root
        );
        assert_eq!(self.root.canonicalize().unwrap(), self.root);
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn global(interpreter: &Interpreter, name: &str, expected: Value) {
    assert_eq!(interpreter.get_global(name), Some(expected), "{name}");
}

#[test]
fn imported_files_have_private_globals_locals_and_fresh_builtins() {
    let project = Project::new();
    project.write(
        "first.solx",
        r#"
        global score = 1
        global str = fn(x) return "module" end
        var private = 42
        exports.read = fn() return score end
        exports.increment = fn() score = score + 1 end
        exports.convert = str
    "#,
    );
    project.write(
        "second.solx",
        r#"
        global score = 20
        exports.read = fn() return score end
        exports.convert = str
    "#,
    );
    let mut interpreter = project.interpreter();
    interpreter
        .run_source(
            r#"
        global score = 100
        import "first.solx"
        import "second.solx"
        first.increment()
        global a = first.read()
        global b = second.read()
        global localConversion = str(5)
        global firstConversion = first.convert(5)
        global secondConversion = second.convert(5)
    "#,
        )
        .unwrap();
    global(&interpreter, "score", Value::Number(100.0));
    global(&interpreter, "a", Value::Number(2.0));
    global(&interpreter, "b", Value::Number(20.0));
    global(&interpreter, "localConversion", Value::String("5".into()));
    global(
        &interpreter,
        "firstConversion",
        Value::String("module".into()),
    );
    global(&interpreter, "secondConversion", Value::String("5".into()));
    for name in ["private", "exports"] {
        assert!(interpreter.run_source(name).is_err());
    }
    assert!(
        interpreter
            .run_source("first.score")
            .unwrap_err()
            .message
            .contains("has no export 'score'")
    );
}

#[test]
fn exports_are_live_but_scalar_export_does_not_alias_a_variable_binding() {
    let project = Project::new();
    project.write(
        "state.solx",
        r#"
        import "object"
        var private = 1
        exports.snapshot = private
        exports.current = private
        exports.items = []
        exports.bump = fn()
            private = private + 1
            exports.current = private
        end
        exports.remove = fn() object.remove(exports, "current") end
        exports.empty = nil
    "#,
    );
    let mut interpreter = project.interpreter();
    interpreter
        .run_source(
            r#"
        import "state.solx"
        import "list"
        var alias = state
        state.bump()
        global snapshot = alias.snapshot
        global current = alias.current
        list.append(alias.items, 9)
        global size = #state.items
        global empty = state.empty
        state.remove()
    "#,
        )
        .unwrap();
    global(&interpreter, "snapshot", Value::Number(1.0));
    global(&interpreter, "current", Value::Number(2.0));
    global(&interpreter, "size", Value::Number(1.0));
    global(&interpreter, "empty", Value::Nil);
    assert!(
        interpreter
            .run_source("state.current")
            .unwrap_err()
            .message
            .contains("has no export 'current'")
    );
    assert_eq!(
        interpreter
            .run_source("state.snapshot = 3")
            .unwrap_err()
            .message,
        "module namespace is read-only"
    );
}

#[test]
fn imported_module_cannot_see_or_assign_callers_locals() {
    let project = Project::new();
    project.write("reader.solx", "exports.value = secret");
    project.write("writer.solx", "secret = 9");
    let mut interpreter = project.interpreter();
    interpreter.run_source("var secret = 42").unwrap();
    for name in ["reader", "writer"] {
        let error = interpreter
            .run_source(&format!("import \"{name}.solx\""))
            .unwrap_err();
        assert!(
            error.message.contains("undeclared variable 'secret'"),
            "{error}"
        );
        assert!(error.message.contains(&format!("{name}.solx")), "{error}");
    }
    interpreter.run_source("global retained = secret").unwrap();
    global(&interpreter, "retained", Value::Number(42.0));
}

#[test]
fn canonical_paths_and_aliases_share_one_cached_module() {
    let project = Project::new();
    project.write(
        "lib/counter.solx",
        r#"
        global count = 0
        exports.next = fn() count = count + 1 return count end
    "#,
    );
    let mut interpreter = project.interpreter();
    interpreter
        .run_source(
            r#"
        import "./lib/counter.solx" as a
        global first = a.next()
        import "lib/../lib/counter.solx" as b
        global same = a == b
        global second = b.next()
    "#,
        )
        .unwrap();
    global(&interpreter, "first", Value::Number(1.0));
    global(&interpreter, "second", Value::Number(2.0));
    global(&interpreter, "same", Value::Boolean(true));
    project.write("lib/counter.solx", "this is now invalid !!!");
    interpreter
        .run_source(r#"import "lib/counter.solx" as c global third = c.next()"#)
        .unwrap();
    global(&interpreter, "third", Value::Number(3.0));
}

#[test]
fn diamond_imports_execute_the_dependency_once() {
    let project = Project::new();
    project.write("shared.solx", "print('loaded') exports.value = 7");
    project.write(
        "left.solx",
        r#"import "shared.solx" exports.shared = shared"#,
    );
    project.write(
        "right.solx",
        r#"import "./shared.solx" exports.shared = shared"#,
    );
    project.write(
        "main.solx",
        r#"import "left.solx" import "right.solx" print(left.shared == right.shared)"#,
    );
    let output = project.cli().arg("main.solx").output().unwrap();
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
        "loaded\ntrue\n"
    );
}

#[test]
fn closure_imports_resolve_from_the_defining_file_after_it_returns() {
    let project = Project::new();
    project.write("value.solx", "exports.value = 100");
    project.write("lib/value.solx", "exports.value = 7");
    project.write(
        "lib/factory.solx",
        r#"
        exports.make = fn()
            var calls = 0
            return fn()
                import "./value.solx"
                calls = calls + 1
                return value.value + calls
            end
        end
    "#,
    );
    let mut interpreter = project.interpreter();
    interpreter
        .run_source(
            r#"
        import "lib/factory.solx"
        var closure = factory.make()
        global first = closure()
        global second = closure()
        import "value.solx"
        global caller = value.value
    "#,
        )
        .unwrap();
    global(&interpreter, "first", Value::Number(8.0));
    global(&interpreter, "second", Value::Number(9.0));
    global(&interpreter, "caller", Value::Number(100.0));
}

#[test]
fn exported_closures_use_defining_globals_even_when_passed_back_and_forth() {
    let project = Project::new();
    project.write(
        "callbacks.solx",
        r#"
        global value = 7
        exports.apply = fn(callback) return callback() + value end
        exports.read = fn() return value end
    "#,
    );
    let mut interpreter = project.interpreter();
    interpreter
        .run_source(
            r#"
        global value = 10
        import "callbacks.solx"
        fn localRead() return value end
        global result = callbacks.apply(localRead)
        fn invoke(f) var value = 100 return f() end
        global other = invoke(callbacks.read)
    "#,
        )
        .unwrap();
    global(&interpreter, "result", Value::Number(17.0));
    global(&interpreter, "other", Value::Number(7.0));
}

#[test]
fn imported_functions_can_use_standard_modules_loops_and_recursion() {
    let project = Project::new();
    project.write(
        "library.solx",
        r#"
        import "math"
        import "list"
        fn fact(n) if n <= 1 then return 1 end return n * fact(n - 1) end
        exports.calculate = fn()
            var values = []
            for i = 1, 3 do list.append(values, fact(i)) end
            return math.max(values[1], values[2], values[3])
        end
    "#,
    );
    let mut interpreter = project.interpreter();
    interpreter
        .run_source(r#"import "library.solx" global result = library.calculate()"#)
        .unwrap();
    global(&interpreter, "result", Value::Number(6.0));
}

#[test]
fn circular_imports_report_the_chain_and_can_be_retried_after_repair() {
    let project = Project::new();
    project.write("a.solx", r#"import "b.solx" exports.value = b.value"#);
    project.write("b.solx", r#"import "a.solx" exports.value = a.value"#);
    let mut interpreter = project.interpreter();
    let error = interpreter.run_source(r#"import "a.solx""#).unwrap_err();
    assert!(error.message.contains("circular import:"), "{error}");
    assert!(
        error.message.contains("a.solx")
            && error.message.contains("b.solx")
            && error.message.contains(" -> "),
        "{error}"
    );
    project.write("b.solx", "exports.value = 9");
    interpreter
        .run_source(r#"import "a.solx" global result = a.value"#)
        .unwrap();
    global(&interpreter, "result", Value::Number(9.0));
}

#[test]
fn direct_and_entry_file_cycles_fail_without_recursing() {
    let project = Project::new();
    project.write("self.solx", r#"import "./self.solx" as myself"#);
    let mut interpreter = project.interpreter();
    let error = interpreter
        .run_source(r#"import "self.solx" as module"#)
        .unwrap_err();
    assert!(error.message.contains("circular import:"), "{error}");
    let entry = project.write("main.solx", r#"import "child.solx""#);
    project.write("child.solx", r#"import "./main.solx""#);
    let error = interpreter.run_file(entry).unwrap_err();
    assert!(error.message.contains("circular import:"), "{error}");
    assert!(error.message.contains("main.solx") && error.message.contains("child.solx"));
}

#[test]
fn missing_invalid_and_unreadable_imports_have_useful_errors() {
    let project = Project::new();
    fs::create_dir(project.root.join("directory.solx")).unwrap();
    fs::write(project.root.join("bytes.solx"), [0xff, 0xfe]).unwrap();
    let mut interpreter = project.interpreter();
    for (source, fragment) in [
        (r#"import "missing.solx""#, "cannot resolve"),
        (r#"import "./file.txt""#, "explicit .solx extension"),
        (r#"import "./folder" as folder"#, "explicit .solx extension"),
        (r#"import "unknown""#, "unknown built-in module 'unknown'"),
        (r#"import "directory.solx""#, "cannot read"),
        (r#"import "bytes.solx""#, "cannot read"),
    ] {
        let error = interpreter.run_source(source).unwrap_err();
        assert!(error.message.contains(fragment), "{source}: {error}");
        assert!(error.message.contains("entry.solx"), "{error}");
    }
    project.write("missing.solx", "exports.value = 1");
    interpreter
        .run_source(r#"import "missing.solx" global result = missing.value"#)
        .unwrap();
    global(&interpreter, "result", Value::Number(1.0));
}

#[test]
fn failed_module_initialization_restores_scopes_globals_and_path_context() {
    let project = Project::new();
    project.write(
        "bad/fail.solx",
        r#"
        import "core"
        global visible = "module"
        exports.partial = 10
        fn fail() if true then core.error("initialization failed") end end
        fail()
    "#,
    );
    project.write("good.solx", "exports.value = 8");
    let mut interpreter = project.interpreter();
    interpreter
        .run_source(
            r#"
        var visible = "file"
        fn caller()
            var visible = "caller"
            if true then import "bad/fail.solx" end
        end
    "#,
        )
        .unwrap();
    let error = interpreter.run_source("caller()").unwrap_err();
    assert!(error.message.contains("initialization failed") && error.message.contains("fail.solx"));
    interpreter
        .run_source(r#"global restored = visible import "good.solx" global value = good.value"#)
        .unwrap();
    global(&interpreter, "restored", Value::String("file".into()));
    global(&interpreter, "value", Value::Number(8.0));
    assert!(interpreter.run_source("fail").is_err());
    project.write("bad/fail.solx", "exports.complete = 20");
    interpreter.run_source(r#"import "bad/fail.solx""#).unwrap();
    assert!(
        interpreter
            .run_source("fail.partial")
            .unwrap_err()
            .message
            .contains("has no export")
    );
    interpreter
        .run_source("global complete = fail.complete")
        .unwrap();
    global(&interpreter, "complete", Value::Number(20.0));
}

#[test]
fn syntax_failures_are_not_cached_and_do_not_execute_partial_source() {
    let project = Project::new();
    project.write("bad.solx", "print('must not run') fn broken(");
    let output = project
        .cli()
        .args(["-e", r#"import "bad.solx""#])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("bad.solx")
    );
    let mut interpreter = project.interpreter();
    let error = interpreter.run_source(r#"import "bad.solx""#).unwrap_err();
    assert!(
        error.message.contains("bad.solx") && error.message.contains("expected a parameter name"),
        "{error}"
    );
    project.write("bad.solx", "exports.value = 12");
    interpreter
        .run_source(r#"import "bad.solx" global result = bad.value"#)
        .unwrap();
    global(&interpreter, "result", Value::Number(12.0));
}

#[test]
fn successful_dependencies_and_side_effects_survive_failed_parent_initialization() {
    let project = Project::new();
    project.write(
        "dependency.solx",
        "exports.count = 0 exports.bump = fn() exports.count = exports.count + 1 end",
    );
    project.write(
        "parent.solx",
        r#"import "dependency.solx" dependency.bump() import "core" core.error("failed")"#,
    );
    let mut interpreter = project.interpreter();
    for _ in 0..2 {
        assert!(interpreter.run_source(r#"import "parent.solx""#).is_err());
    }
    interpreter
        .run_source(r#"import "dependency.solx" global count = dependency.count"#)
        .unwrap();
    global(&interpreter, "count", Value::Number(2.0));
}

#[test]
fn exported_function_errors_preserve_modules_and_restore_the_importer() {
    let project = Project::new();
    project.write(
        "bad.solx",
        r#"
        import "core"
        exports.count = 0
        exports.fail = fn()
            exports.count = exports.count + 1
            var visible = "module"
            core.error("call failed")
        end
    "#,
    );
    let mut interpreter = project.interpreter();
    interpreter
        .run_source(r#"var visible = "file" import "bad.solx""#)
        .unwrap();
    let error = interpreter.run_source("bad.fail()").unwrap_err();
    assert!(error.message.contains("bad.solx") && error.message.contains("call failed"));
    interpreter
        .run_source("global restored = visible global count = bad.count")
        .unwrap();
    global(&interpreter, "restored", Value::String("file".into()));
    global(&interpreter, "count", Value::Number(1.0));
}

#[test]
fn different_interpreters_have_independent_module_instances() {
    let project = Project::new();
    project.write(
        "state.solx",
        "exports.count = 0 exports.bump = fn() exports.count = exports.count + 1 end",
    );
    let mut first = project.interpreter();
    let mut second = project.interpreter();
    first
        .run_source(r#"import "state.solx" state.bump() global count = state.count"#)
        .unwrap();
    second
        .run_source(r#"import "state.solx" global count = state.count"#)
        .unwrap();
    global(&first, "count", Value::Number(1.0));
    global(&second, "count", Value::Number(0.0));
}

#[test]
fn import_bindings_respect_constants_and_block_scope() {
    let project = Project::new();
    project.write("value.solx", "exports.answer = 42");
    let mut interpreter = project.interpreter();
    interpreter
        .run_source(
            r#"
        var captured
        if true then
            import "value.solx" as localModule
            captured = fn() return localModule.answer end
        end
        global answer = captured()
    "#,
        )
        .unwrap();
    global(&interpreter, "answer", Value::Number(42.0));
    assert!(interpreter.run_source("localModule").is_err());
    assert_eq!(
        interpreter
            .run_source(r#"const fixed = 1 import "value.solx" as fixed"#)
            .unwrap_err()
            .message,
        "cannot redeclare constant 'fixed'"
    );
    project.write("rebind.solx", "exports = {}");
    let error = interpreter
        .run_source(r#"import "rebind.solx""#)
        .unwrap_err();
    assert!(
        error.message.contains("cannot reassign constant 'exports'"),
        "{error}"
    );
}

#[test]
fn absolute_imports_and_filenames_requiring_aliases_work() {
    let project = Project::new();
    let module = project.write("my module.solx", "exports.value = 6");
    // Forward slashes avoid introducing Solx string escape sequences on Windows.
    let specifier = module.to_str().unwrap().replace('\\', "/");
    let mut interpreter = Interpreter::new();
    interpreter
        .run_source(&format!(
            "import \"{specifier}\" as module global result = module.value"
        ))
        .unwrap();
    global(&interpreter, "result", Value::Number(6.0));
}

#[test]
fn cli_entry_paths_are_resolved_independently_of_import_paths() {
    let project = Project::new();
    project.write("value.solx", "exports.value = 100");
    project.write("app/value.solx", "exports.value = 7");
    project.write(
        "app/main.solx",
        r#"import "./value.solx" print(value.value)"#,
    );
    let output = project.cli().arg("app/main.solx").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "7");
}

#[test]
fn execute_and_plain_repl_imports_use_the_initial_working_directory() {
    let project = Project::new();
    project.write("value.solx", "exports.value = 7");
    let output = project
        .cli()
        .args(["-e", r#"import "value.solx" print(value.value)"#])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "7");
    let mut child = project
        .cli()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"import \"value.solx\"\nprint(value.value)\nquit\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n")
            .contains("> 7\n")
    );
}

#[test]
fn interactive_mode_after_an_entry_uses_its_directory_and_cached_imports() {
    let project = Project::new();
    project.write("app/value.solx", "exports.value = 9");
    project.write("app/main.solx", r#"import "value.solx""#);
    let mut child = project
        .cli()
        .args(["-i", "app/main.solx"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"import \"value.solx\" as again\nprint(again == value)\nprint(again.value)\nquit\n",
        )
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
    assert!(stdout.contains("> true\n> 9\n"), "{stdout}");
}

#[test]
fn explicit_entry_runs_are_not_import_cache_hits() {
    let project = Project::new();
    let entry = project.write("main.solx", "runs = runs + 1");
    let mut interpreter = Interpreter::new();
    interpreter.run_source("global runs = 0").unwrap();
    interpreter.run_file(&entry).unwrap();
    interpreter.run_file(&entry).unwrap();
    global(&interpreter, "runs", Value::Number(2.0));
}

#[cfg(unix)]
#[test]
fn symlink_paths_share_the_canonical_cache_identity() {
    use std::os::unix::fs::symlink;
    let project = Project::new();
    let target = project.write("value.solx", "exports.value = 7");
    symlink(&target, project.root.join("alias.solx")).unwrap();
    let mut interpreter = project.interpreter();
    interpreter
        .run_source(r#"import "value.solx" import "alias.solx" global same = value == alias"#)
        .unwrap();
    global(&interpreter, "same", Value::Boolean(true));
}

#[test]
fn documented_module_example_runs_from_an_unrelated_working_directory() {
    let project = Project::new();
    let entry = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/modules/main.solx");
    let output = project.cli().arg(entry).output().unwrap();
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
        "mean: 20\nsorted: [10, 20, 30]\n"
    );
}
