use solx::interpreter::Interpreter;
use solx::limits::{DEFAULT_MAX_SOURCE_BYTES, ExecutionLimits, LimitKind};
use solx::runtime::{RuntimeError, Value, error::RuntimeErrorKind};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

fn limited(max_source_bytes: usize) -> Interpreter {
    Interpreter::with_limits(ExecutionLimits {
        max_source_bytes,
        ..ExecutionLimits::default()
    })
    .unwrap()
}

fn source_limit(error: &RuntimeError) {
    assert_eq!(
        error.kind,
        RuntimeErrorKind::LimitExceeded(LimitKind::SourceBytes),
        "{error}"
    );
    assert!(!error.is_catchable());
}

struct Project {
    root: PathBuf,
    base: PathBuf,
}
impl Project {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().canonicalize().unwrap();
        let root = base.join(format!(
            "solx-source-limits-{}-{}-{}",
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
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, bytes).unwrap();
        path
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
                .starts_with("solx-source-limits-")
        );
        fs::remove_dir_all(checked).unwrap();
    }
}

#[test]
fn source_buffers_enforce_utf8_byte_boundaries_before_parsing_or_effects() {
    let source = "--\u{e9}";
    limited(source.len()).run_source(source).unwrap();
    source_limit(&limited(source.len() - 1).run_source(source).unwrap_err());
    let mut interpreter = limited(32);
    interpreter.run_source("global retained = true").unwrap();
    source_limit(
        &interpreter
            .run_source(&format!("global forbidden = true{}", " ".repeat(32)))
            .unwrap_err(),
    );
    source_limit(&interpreter.run_source(&"@".repeat(33)).unwrap_err());
    assert!(interpreter.get_global("forbidden").is_none());
    assert_eq!(
        interpreter.get_global("retained"),
        Some(Value::Boolean(true))
    );
    interpreter.run_source("global recovered = true").unwrap();
}

#[test]
fn empty_input_and_host_raised_budgets_are_supported_without_an_artificial_ceiling() {
    let mut empty = limited(0);
    empty.run_source("").unwrap();
    source_limit(&empty.run_source(" ").unwrap_err());
    // A host-supplied AST has no source byte representation; its AST/depth limits remain.
    empty
        .run_program(&solx::parser::parse("global okay = true").unwrap())
        .unwrap();
    limited(usize::MAX)
        .run_source("global okay = true")
        .unwrap();
}

#[test]
fn files_use_the_default_one_mib_boundary_and_do_not_run_an_oversized_prefix() {
    assert_eq!(
        ExecutionLimits::default().max_source_bytes,
        DEFAULT_MAX_SOURCE_BYTES
    );
    let project = Project::new();
    let exact = project.write(
        "exact.solx",
        format!("--{}", "x".repeat(DEFAULT_MAX_SOURCE_BYTES - 2)),
    );
    Interpreter::new().run_file(exact).unwrap();
    let oversized = project.write(
        "large.solx",
        format!(
            "global forbidden = true\n--{}",
            "x".repeat(DEFAULT_MAX_SOURCE_BYTES)
        ),
    );
    let mut interpreter = Interpreter::new();
    let error = interpreter.run_file(&oversized).unwrap_err();
    source_limit(&error);
    assert_eq!(
        error.location.unwrap().file.as_deref(),
        Some(oversized.canonicalize().unwrap().as_path())
    );
    assert!(interpreter.get_global("forbidden").is_none());
}

#[test]
fn imports_share_the_file_limit_bypass_handlers_and_remain_retryable() {
    let maximum = 160;
    let project = Project::new();
    let entry = project.write("entry.solx", "try import './module.solx' as loaded global okay = loaded.okay catch err global caught = true end");
    let module = project.write("module.solx", "x".repeat(maximum + 1));
    let mut interpreter = limited(maximum);
    let error = interpreter.run_file(&entry).unwrap_err();
    source_limit(&error);
    assert_eq!(
        error.location.as_ref().unwrap().file.as_deref(),
        Some(module.canonicalize().unwrap().as_path())
    );
    assert!(
        error
            .contexts
            .iter()
            .any(|context| context.description.contains("import './module.solx'"))
    );
    assert!(interpreter.get_global("caught").is_none());
    assert!(interpreter.get_global("okay").is_none());
    let source = "exports.okay = true\n--";
    project.write(
        "module.solx",
        format!("{source}{}", "x".repeat(maximum - source.len())),
    );
    // Each file fits independently even though total loaded bytes exceed the limit.
    interpreter.run_file(entry).unwrap();
    assert_eq!(interpreter.get_global("okay"), Some(Value::Boolean(true)));
}

#[test]
fn file_read_errors_keep_paths_and_utf8_validation_occurs_after_the_size_check() {
    let project = Project::new();
    let invalid = project.write("invalid.solx", [0xff]);
    let error = limited(1).run_file(&invalid).unwrap_err();
    assert!(error.is_catchable());
    assert!(error.message.contains("valid UTF-8"));
    assert_eq!(
        error.location.unwrap().file.as_deref(),
        Some(invalid.canonicalize().unwrap().as_path())
    );
    source_limit(&limited(0).run_file(&invalid).unwrap_err());
    limited(0)
        .run_file(project.write("empty.solx", ""))
        .unwrap();
}

#[test]
fn cli_source_limit_applies_to_execute_and_file_inputs() {
    let source = "print('\u{e9}')";
    let exact = source.len().to_string();
    let too_small = (source.len() - 1).to_string();
    let project = Project::new();
    let file = project.write("main.solx", source);
    for arguments in [vec!["-e", source], vec![file.to_str().unwrap()]] {
        let output = Command::new(env!("CARGO_BIN_EXE_solx"))
            .args(["--max-source-bytes", &exact])
            .args(&arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(env!("CARGO_BIN_EXE_solx"))
            .args(["--max-source-bytes", &too_small])
            .args(&arguments)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("source byte limit exceeded"));
    }
}

#[test]
fn repl_discards_oversized_lines_and_multiline_buffers_then_recovers() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args(["--max-source-bytes", "32"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = format!(
        "{}print('tail')\nif true then\nglobal forbidden = true\nprint('okay')\nquit\n",
        "x".repeat(1000)
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("okay"), "{stdout}");
    assert!(!stdout.contains("tail"), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("source byte limit exceeded (32)").count(),
        2,
        "{stderr}"
    );
}
