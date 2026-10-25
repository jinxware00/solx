use solx::host::HostCapabilities;
use solx::interpreter::Interpreter;
use solx::limits::{ExecutionLimits, LimitKind};
use solx::runtime::error::RuntimeErrorKind;
use solx::runtime::{RuntimeError, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const MAX_BYTES: usize = 1024 * 1024;

struct Project {
    root: PathBuf,
    base: PathBuf,
}
impl Project {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!(
            "solx-host-libraries-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        Self { root, base }
    }
    fn interpreter(&self, read: bool, write: bool) -> Interpreter {
        let mut interpreter = Interpreter::new();
        interpreter.set_capabilities(
            HostCapabilities::default()
                .with_filesystem(&self.root, read, write)
                .unwrap(),
        );
        interpreter
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let checked = self.root.canonicalize().unwrap();
        assert_eq!(checked.parent(), Some(self.base.as_path()));
        assert!(
            checked
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("solx-host-libraries-")
        );
        fs::remove_dir_all(checked).unwrap();
    }
}

fn call(interpreter: &mut Interpreter, expression: &str) -> Result<Value, RuntimeError> {
    interpreter.run_source(&format!(
        "import 'fs' import 'path' import 'time' global result = {expression}"
    ))?;
    Ok(interpreter.get_global("result").unwrap())
}

fn error(interpreter: &mut Interpreter, expression: &str, expected: &str) {
    let failure = call(interpreter, expression).unwrap_err();
    assert!(
        failure.message.contains(expected),
        "{expression}: {failure}"
    );
}

fn text(text: &str) -> Value {
    Value::String(text.into())
}

fn host_value(interpreter: &mut Interpreter, value: Value) {
    interpreter.run_source("global host = []").unwrap();
    let Value::List(ref slot) = interpreter.get_global("host").unwrap() else {
        unreachable!()
    };
    slot.borrow_mut().push(value);
}

#[test]
fn filesystem_permissions_default_to_denied_and_importing_has_no_effect() {
    let mut interpreter = Interpreter::new();
    interpreter.run_source("import 'fs' import 'time'").unwrap();
    for expression in [
        "fs.read_text('a')",
        "fs.exists('.')",
        "fs.is_file('a')",
        "fs.is_dir('.')",
        "fs.read_dir('.')",
    ] {
        error(
            &mut interpreter,
            expression,
            "requires filesystem read permission",
        );
    }
    for expression in [
        "fs.write_text('a', '')",
        "fs.append_text('a', '')",
        "fs.create_dir('a')",
        "fs.remove_file('a')",
        "fs.remove_dir('a')",
        "fs.rename('a', 'b')",
    ] {
        error(
            &mut interpreter,
            expression,
            "requires filesystem write permission",
        );
    }
    error(
        &mut interpreter,
        "time.unix_ms()",
        "requires clock permission",
    );
    error(
        &mut interpreter,
        "time.elapsed_ms()",
        "requires clock permission",
    );
}

#[test]
fn read_and_write_grants_are_independent_and_cached_functions_can_be_revoked() {
    let project = Project::new();
    fs::write(project.root.join("a"), "original").unwrap();
    let mut reader = project.interpreter(true, false);
    assert_eq!(
        call(&mut reader, "fs.read_text('a')").unwrap(),
        text("original")
    );
    for expression in [
        "fs.write_text('a', 'bad')",
        "fs.append_text('a', 'bad')",
        "fs.remove_file('a')",
        "fs.create_dir('b')",
        "fs.remove_dir('b')",
        "fs.rename('a', 'b')",
    ] {
        error(&mut reader, expression, "write permission");
    }
    assert_eq!(
        fs::read_to_string(project.root.join("a")).unwrap(),
        "original"
    );
    let mut writer = project.interpreter(false, true);
    call(&mut writer, "fs.write_text('a', 'new')").unwrap();
    for expression in [
        "fs.read_text('a')",
        "fs.exists('a')",
        "fs.is_file('a')",
        "fs.is_dir('.')",
        "fs.read_dir('.')",
    ] {
        error(&mut writer, expression, "read permission");
    }
    reader.run_source("global saved = fs.read_text").unwrap();
    reader.set_capabilities(HostCapabilities::default());
    error(&mut reader, "saved('a')", "read permission");
}

#[test]
fn text_round_trip_append_truncate_unicode_and_directory_lifecycle() {
    let project = Project::new();
    let mut interpreter = project.interpreter(true, true);
    // Use literal Unicode and preserve NUL/newline bytes in file contents.
    interpreter
        .run_source("import 'fs' import 'core'\n")
        .unwrap();
    call(&mut interpreter, "fs.create_dir('data')").unwrap();
    host_value(&mut interpreter, text("h\u{e9}\0\r\n"));
    assert_eq!(
        call(
            &mut interpreter,
            "fs.write_text('data/caf\u{e9}.txt', host[1])"
        )
        .unwrap(),
        Value::Nil
    );
    call(
        &mut interpreter,
        "fs.append_text('data/caf\u{e9}.txt', '!')",
    )
    .unwrap();
    assert_eq!(
        call(&mut interpreter, "fs.read_text('data/caf\u{e9}.txt')").unwrap(),
        text("h\u{e9}\0\r\n!")
    );
    call(
        &mut interpreter,
        "fs.rename('data/caf\u{e9}.txt', 'data/final')",
    )
    .unwrap();
    assert_eq!(
        call(&mut interpreter, "fs.is_file('data/final')").unwrap(),
        Value::Boolean(true)
    );
    assert_eq!(
        call(&mut interpreter, "fs.is_dir('data')").unwrap(),
        Value::Boolean(true)
    );
    assert_eq!(
        call(&mut interpreter, "fs.is_file('data')").unwrap(),
        Value::Boolean(false)
    );
    assert_eq!(
        call(&mut interpreter, "fs.exists('data/missing')").unwrap(),
        Value::Boolean(false)
    );
    error(&mut interpreter, "fs.remove_dir('data')", "fs.remove_dir()");
    call(&mut interpreter, "fs.write_text('data/final', '')").unwrap();
    assert_eq!(
        call(&mut interpreter, "fs.read_text('data/final')").unwrap(),
        text("")
    );
    call(&mut interpreter, "fs.remove_file('data/final')").unwrap();
    call(&mut interpreter, "fs.remove_dir('data')").unwrap();
    assert_eq!(
        call(&mut interpreter, "fs.exists('data')").unwrap(),
        Value::Boolean(false)
    );
}

#[test]
fn paths_cannot_escape_the_granted_root_or_select_windows_streams() {
    let project = Project::new();
    let mut interpreter = project.interpreter(true, true);
    for path in [
        "",
        "..",
        "../secret",
        "sub/../secret",
        "/absolute",
        "C:/secret",
        "file:stream",
        "nul\0name",
    ] {
        host_value(&mut interpreter, text(path));
        for expression in [
            "fs.read_text(host[1])",
            "fs.write_text(host[1], 'bad')",
            "fs.read_dir(host[1])",
            "fs.exists(host[1])",
            "fs.create_dir(host[1])",
            "fs.rename('a', host[1])",
            "fs.rename(host[1], 'a')",
        ] {
            error(&mut interpreter, expression, "argument");
        }
    }
    host_value(&mut interpreter, text(&"x".repeat(4097)));
    error(&mut interpreter, "fs.exists(host[1])", "4096");
    #[cfg(windows)]
    for path in [
        r"\absolute",
        r"\\server\share\file",
        r"..\secret",
        r"\\?\C:\secret",
    ] {
        host_value(&mut interpreter, text(path));
        error(&mut interpreter, "fs.read_text(host[1])", "relative path");
    }
    assert_eq!(fs::read_dir(&project.root).unwrap().count(), 0);
}

#[test]
fn text_limits_validate_before_truncation_and_reject_invalid_utf8() {
    let project = Project::new();
    let mut interpreter = project.interpreter(true, true);
    let exact = "\u{e9}".repeat(MAX_BYTES / 2);
    host_value(&mut interpreter, text(&exact));
    call(&mut interpreter, "fs.write_text('a', host[1])").unwrap();
    assert_eq!(
        call(&mut interpreter, "fs.read_text('a')").unwrap(),
        text(&exact)
    );
    host_value(&mut interpreter, text(&(exact + "!")));
    error(&mut interpreter, "fs.write_text('a', host[1])", "1048576");
    error(&mut interpreter, "fs.append_text('a', host[1])", "1048576");
    assert_eq!(
        fs::metadata(project.root.join("a")).unwrap().len(),
        MAX_BYTES as u64
    );
    error(
        &mut interpreter,
        "fs.write_text('absent', host[1])",
        "1048576",
    );
    assert!(!project.root.join("absent").exists());
    call(&mut interpreter, "fs.append_text('a', '!')").unwrap();
    error(&mut interpreter, "fs.read_text('a')", "1048576");
    fs::write(project.root.join("a"), [0xff]).unwrap();
    error(&mut interpreter, "fs.read_text('a')", "valid UTF-8");
    call(&mut interpreter, "fs.remove_file('a')").unwrap(); // Failed reads closed the handle.
}

#[test]
fn directory_results_are_sorted_bounded_and_not_recursive() {
    let project = Project::new();
    for name in ["z", "a", "\u{e9}"] {
        fs::write(project.root.join(name), "").unwrap();
    }
    fs::create_dir(project.root.join("nested")).unwrap();
    fs::write(project.root.join("nested/hidden"), "").unwrap();
    let mut interpreter = project.interpreter(true, false);
    assert_eq!(
        call(&mut interpreter, "fs.read_dir('.')").unwrap(),
        Value::list(vec![text("a"), text("nested"), text("z"), text("\u{e9}")])
    );
    for index in 4..4096 {
        fs::write(project.root.join(format!("item-{index:04}")), "").unwrap();
    }
    let Value::List(ref entries) = call(&mut interpreter, "fs.read_dir('.')").unwrap() else {
        unreachable!()
    };
    assert_eq!(entries.borrow().len(), 4096);
    fs::write(project.root.join("overflow"), "").unwrap();
    error(&mut interpreter, "fs.read_dir('.')", "4096 entries");
}

#[test]
fn arity_and_types_are_checked_before_io_permissions() {
    let mut interpreter = Interpreter::new();
    for (expression, message) in [
        ("fs.write_text(1)", "expects 2 argument(s)"),
        ("fs.read_text(1)", "argument 1 requires String"),
        ("fs.write_text('a', 1)", "argument 2 requires String"),
        ("fs.rename('a', nil)", "argument 2 requires String"),
        ("path.join('a')", "expects 2 argument(s)"),
        ("path.basename(false)", "argument 1 requires String"),
        ("time.unix_ms(1)", "expects 0 argument(s)"),
    ] {
        error(&mut interpreter, expression, message);
    }
}

#[test]
fn filesystem_failures_are_catchable_and_preserve_completed_effects() {
    let project = Project::new();
    let mut interpreter = project.interpreter(true, true);
    interpreter.run_source("import 'fs' global caught = false\ntry fs.write_text('done', 'ok') fs.read_text('absent') catch err caught = true end").unwrap();
    assert_eq!(interpreter.get_global("caught"), Some(Value::Boolean(true)));
    assert_eq!(fs::read_to_string(project.root.join("done")).unwrap(), "ok");
}

#[test]
fn imported_modules_use_the_callers_current_grants() {
    let project = Project::new();
    fs::write(project.root.join("data"), "inside").unwrap();
    fs::write(
        project.root.join("reader.solx"),
        "import 'fs' exports.read = fn() return fs.read_text('data') end",
    )
    .unwrap();
    let mut interpreter = project.interpreter(true, false);
    fs::write(
        project.root.join("main.solx"),
        "import './reader.solx' as reader global read = reader.read global value = read()",
    )
    .unwrap();
    interpreter
        .run_file(project.root.join("main.solx"))
        .unwrap();
    assert_eq!(interpreter.get_global("value"), Some(text("inside")));
    interpreter.set_capabilities(HostCapabilities::default());
    error(&mut interpreter, "read()", "read permission");
}

#[test]
fn zero_deadline_prevents_file_creation() {
    let project = Project::new();
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_duration: Some(std::time::Duration::ZERO),
        ..ExecutionLimits::default()
    })
    .unwrap();
    interpreter.set_capabilities(
        HostCapabilities::default()
            .with_filesystem(&project.root, false, true)
            .unwrap(),
    );
    let failure = call(&mut interpreter, "fs.write_text('forbidden', 'bad')").unwrap_err();
    assert_eq!(
        failure.kind,
        RuntimeErrorKind::LimitExceeded(LimitKind::Deadline)
    );
    assert!(!failure.is_catchable());
    assert!(!project.root.join("forbidden").exists());
}

#[test]
fn io_loops_share_steps_and_cancellation_prevents_side_effects() {
    let project = Project::new();
    fs::write(project.root.join("large"), vec![b'a'; MAX_BYTES]).unwrap();
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_steps: Some(80),
        ..ExecutionLimits::default()
    })
    .unwrap();
    interpreter.set_capabilities(
        HostCapabilities::default()
            .with_filesystem(&project.root, true, true)
            .unwrap(),
    );
    let failure = call(&mut interpreter, "fs.read_text('large')").unwrap_err();
    assert_eq!(
        failure.kind,
        RuntimeErrorKind::LimitExceeded(LimitKind::Steps)
    );
    assert!(!failure.is_catchable());
    call(&mut interpreter, "fs.remove_file('large')").unwrap();
    host_value(&mut interpreter, text(&"x".repeat(MAX_BYTES)));
    let failure = call(&mut interpreter, "fs.write_text('partial', host[1])").unwrap_err();
    assert_eq!(
        failure.kind,
        RuntimeErrorKind::LimitExceeded(LimitKind::Steps)
    );
    let written = fs::metadata(project.root.join("partial")).unwrap().len();
    assert!(written > 0 && written < MAX_BYTES as u64);
    call(&mut interpreter, "fs.remove_file('partial')").unwrap();
    let token = interpreter.cancellation_token();
    token.cancel();
    let failure = call(&mut interpreter, "fs.write_text('forbidden', 'bad')").unwrap_err();
    assert!(!failure.is_catchable());
    assert!(!project.root.join("forbidden").exists());
    token.reset();
    call(&mut interpreter, "fs.write_text('okay', 'ok')").unwrap();
}

#[test]
fn interpreter_roots_are_independent_and_do_not_change_the_working_directory() {
    let first = Project::new();
    let second = Project::new();
    let cwd = std::env::current_dir().unwrap();
    let mut a = first.interpreter(true, true);
    let mut b = second.interpreter(true, true);
    call(&mut a, "fs.write_text('same', 'first')").unwrap();
    call(&mut b, "fs.write_text('same', 'second')").unwrap();
    assert_eq!(call(&mut a, "fs.read_text('same')").unwrap(), text("first"));
    assert_eq!(
        call(&mut b, "fs.read_text('same')").unwrap(),
        text("second")
    );
    assert_eq!(std::env::current_dir().unwrap(), cwd);
}

#[cfg(any(unix, windows))]
fn symlink_file(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link).unwrap();
}

#[cfg(any(unix, windows))]
#[test]
fn symlinks_cannot_read_or_overwrite_outside_the_granted_directory() {
    let project = Project::new();
    let outside = Project::new();
    fs::write(outside.root.join("secret"), "unchanged").unwrap();
    symlink_file(&outside.root.join("secret"), &project.root.join("escape"));
    fs::write(project.root.join("inside"), "allowed").unwrap();
    symlink_file(Path::new("inside"), &project.root.join("internal"));
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside.root, project.root.join("outside")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&outside.root, project.root.join("outside")).unwrap();
    let mut interpreter = project.interpreter(true, true);
    for expression in [
        "fs.read_text('escape')",
        "fs.write_text('escape', 'bad')",
        "fs.append_text('escape', 'bad')",
        "fs.exists('escape')",
    ] {
        error(&mut interpreter, expression, "fs.");
    }
    for expression in [
        "fs.read_dir('outside')",
        "fs.create_dir('outside/new')",
        "fs.remove_file('outside/secret')",
        "fs.rename('inside', 'outside/stolen')",
        "fs.rename('outside/secret', 'stolen')",
    ] {
        error(&mut interpreter, expression, "fs.");
    }
    assert_eq!(
        call(&mut interpreter, "fs.read_text('internal')").unwrap(),
        text("allowed")
    );
    assert_eq!(
        fs::read_to_string(outside.root.join("secret")).unwrap(),
        "unchanged"
    );
    // Removal operates on the symlink itself and must leave its target intact.
    call(&mut interpreter, "fs.remove_file('escape')").unwrap();
    assert!(outside.root.join("secret").exists());
}

// macOS filesystems reject this filename at creation, before Solx can read it.
#[cfg(target_os = "linux")]
#[test]
fn invalid_filename_encoding_is_rejected_by_directory_listing() {
    use std::os::unix::ffi::OsStringExt;
    let project = Project::new();
    let mut interpreter = project.interpreter(true, true);
    fs::write(
        project.root.join(std::ffi::OsString::from_vec(vec![0xff])),
        "",
    )
    .unwrap();
    error(
        &mut interpreter,
        "fs.read_dir('.')",
        "filename is not valid UTF-8",
    );
}

#[cfg(unix)]
#[test]
fn special_files_fail_without_waiting_for_a_fifo_peer() {
    let project = Project::new();
    let mut interpreter = project.interpreter(true, true);
    assert!(
        Command::new("mkfifo")
            .arg(project.root.join("pipe"))
            .status()
            .unwrap()
            .success()
    );
    error(&mut interpreter, "fs.read_text('pipe')", "regular file");
    error(
        &mut interpreter,
        "fs.write_text('pipe', 'bad')",
        "fs.write_text()",
    );
}

#[test]
fn path_helpers_use_platform_rules_without_touching_the_filesystem() {
    let mut interpreter = Interpreter::new();
    for (expression, expected) in [
        ("path.basename('a/b.txt')", text("b.txt")),
        ("path.dirname('a/b.txt')", text("a")),
        ("path.dirname('file')", text("")),
        ("path.dirname('')", Value::Nil),
        ("path.extension('archive.tar.gz')", text("gz")),
        ("path.stem('archive.tar.gz')", text("archive.tar")),
        ("path.extension('.hidden')", Value::Nil),
        ("path.extension('file.')", text("")),
        ("path.basename('..')", Value::Nil),
        ("path.is_absolute('relative')", Value::Boolean(false)),
    ] {
        assert_eq!(
            call(&mut interpreter, expression).unwrap(),
            expected,
            "{expression}"
        );
    }
    assert_eq!(
        call(&mut interpreter, "path.join('missing', 'file')").unwrap(),
        text(Path::new("missing").join("file").to_str().unwrap())
    );
    let absolute = if cfg!(windows) {
        "C:/absolute/file"
    } else {
        "/absolute/file"
    };
    host_value(&mut interpreter, text(absolute));
    assert_eq!(
        call(&mut interpreter, "path.is_absolute(host[1])").unwrap(),
        Value::Boolean(true)
    );
    assert_eq!(
        call(&mut interpreter, "path.join('base', host[1])").unwrap(),
        text(absolute)
    );
    host_value(&mut interpreter, text(&"x".repeat(4096)));
    error(
        &mut interpreter,
        "path.join(host[1], 'x')",
        "output exceeds 4096",
    );
    host_value(&mut interpreter, text("bad\0path"));
    error(&mut interpreter, "path.basename(host[1])", "NUL");
}

#[test]
fn clocks_return_exact_milliseconds_and_grants_remain_isolated() {
    let mut interpreter = Interpreter::new();
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    interpreter.set_capabilities(HostCapabilities::default().with_time());
    let Value::Integer(now) = call(&mut interpreter, "time.unix_ms()").unwrap() else {
        panic!("expected exact integer")
    };
    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    assert!((before..=after).contains(&(now as u128)));
    let Value::Integer(first) = call(&mut interpreter, "time.elapsed_ms()").unwrap() else {
        panic!("expected exact integer")
    };
    let Value::Integer(second) = call(&mut interpreter, "time.elapsed_ms()").unwrap() else {
        panic!("expected exact integer")
    };
    assert!(first >= 0 && second >= first);
    error(
        &mut Interpreter::new(),
        "time.elapsed_ms()",
        "clock permission",
    );
    interpreter.set_capabilities(HostCapabilities::default());
    error(&mut interpreter, "time.unix_ms()", "clock permission");
}

#[test]
fn cli_requires_grants_and_resolves_paths_against_the_selected_root() {
    let project = Project::new();
    let binary = env!("CARGO_BIN_EXE_solx");
    let source = "import 'fs' import 'time' fs.write_text('cli.txt', 'ok') print(fs.read_text('cli.txt'), time.elapsed_ms() >= 0)";
    let denied = Command::new(binary)
        .args(["--fs-root"])
        .arg(&project.root)
        .args(["-e", source])
        .output()
        .unwrap();
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("write permission"));
    assert!(!project.root.join("cli.txt").exists());
    let allowed = Command::new(binary)
        .args(["--allow-read", "--allow-write", "--allow-time", "--fs-root"])
        .arg(&project.root)
        .args(["-e", source])
        .output()
        .unwrap();
    assert!(
        allowed.status.success(),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    assert_eq!(
        String::from_utf8(allowed.stdout).unwrap().trim(),
        "ok\ttrue"
    );
    let default_root = Command::new(binary)
        .current_dir(&project.root)
        .args([
            "--allow-read",
            "-e",
            "import 'fs' print(fs.read_text('cli.txt'))",
        ])
        .output()
        .unwrap();
    assert!(default_root.status.success());
    assert_eq!(String::from_utf8(default_root.stdout).unwrap().trim(), "ok");
    let invalid_root = Command::new(binary)
        .args(["--allow-read", "--fs-root"])
        .arg(project.root.join("missing"))
        .args(["-e", "print('never')"])
        .output()
        .unwrap();
    assert!(!invalid_root.status.success());
    assert!(invalid_root.stdout.is_empty());
    let checked = Command::new(binary)
        .args(["check", "--json"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/file_io.solx"
        ))
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "{}",
        String::from_utf8_lossy(&checked.stdout)
    );
}
