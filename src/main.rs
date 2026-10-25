use clap::{Args, Parser, Subcommand};
use std::{
    io::{self, BufRead, Read, Write},
    path::PathBuf,
    process::ExitCode,
    time::Duration,
};

use solx::{
    host::HostCapabilities,
    interpreter::Interpreter,
    lexer::{TokenKind, tokenize},
    limits::{
        DEFAULT_MAX_FORMAT_BYTES, DEFAULT_MAX_FORMAT_WORK, DEFAULT_MAX_SOURCE_BYTES,
        DEFAULT_MAX_STEPS, ExecutionLimits, MAX_CALL_DEPTH, MAX_IMPORT_DEPTH,
    },
    parser::{ParseError, parse_with_limits},
};

#[derive(Debug, Parser)]
#[command(
    name = "solx",
    version,
    about = "The Solx programming language",
    long_about = "Solx: a programming language focused on performance and low memory usage.",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Execute source code passed directly on the command line
    #[arg(short = 'e', long = "execute", value_name = "CODE")]
    execute: Option<String>,

    /// Enter interactive mode after executing code or a file
    #[arg(short = 'i', long = "interactive")]
    interactive: bool,

    /// Allow fs reads and directory queries inside --fs-root
    #[arg(long)]
    allow_read: bool,

    /// Allow fs writes, creation, rename, and removal inside --fs-root
    #[arg(long)]
    allow_write: bool,

    /// Root for fs operations (default: current directory); grants no access alone
    #[arg(long, value_name = "DIR")]
    fs_root: Option<PathBuf>,

    /// Allow time.unix_ms() and time.elapsed_ms()
    #[arg(long)]
    allow_time: bool,

    /// Maximum evaluation steps per file, -e command, or REPL buffer
    #[arg(long, value_name = "N", default_value_t = DEFAULT_MAX_STEPS)]
    max_steps: u64,

    /// Disable the step budget; depth limits and Ctrl+C still apply
    #[arg(long, conflicts_with = "max_steps")]
    unlimited_steps: bool,

    /// Cooperative elapsed-time limit per run in milliseconds; omitted disables it
    #[arg(long, value_name = "MS")]
    timeout_ms: Option<u64>,

    /// Maximum UTF-8 bytes per source buffer or file, including each imported file
    #[arg(long, value_name = "N", default_value_t = DEFAULT_MAX_SOURCE_BYTES)]
    max_source_bytes: usize,

    /// Maximum UTF-8 bytes per str(), concatenation, or complete print line
    #[arg(long, value_name = "N", default_value_t = DEFAULT_MAX_FORMAT_BYTES)]
    max_format_bytes: usize,

    /// Maximum formatting traversal/copy/comparison work per run
    #[arg(long, value_name = "N", default_value_t = DEFAULT_MAX_FORMAT_WORK)]
    max_format_work: u64,

    /// Maximum simultaneous user-function calls (1..32); combined depth also applies
    #[arg(long, value_name = "N", default_value_t = MAX_CALL_DEPTH)]
    max_call_depth: usize,

    /// Maximum simultaneous uncached imports (1..16), excluding the entry file
    #[arg(long, value_name = "N", default_value_t = MAX_IMPORT_DEPTH)]
    max_import_depth: usize,

    /// Solx source file to execute
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check syntax without executing code or resolving imports
    Check(CheckArgs),
}

#[derive(Debug, Args)]
struct CheckArgs {
    /// Write one schema-versioned JSON result to stdout
    #[arg(long)]
    json: bool,

    /// Also report names without a declaration in this buffer's lexical scopes
    #[arg(long)]
    names: bool,

    /// Include declaration symbols with exact UTF-8 ranges in JSON output
    #[arg(long, requires = "json")]
    symbols: bool,

    /// Read the current source buffer from stdin (up to 1 MiB)
    #[arg(long, conflicts_with = "file")]
    stdin: bool,

    /// UTF-8 source file to check (up to 1 MiB)
    #[arg(value_name = "FILE", required_unless_present = "stdin")]
    file: Option<PathBuf>,
}

fn main() -> ExitCode {
    let mut cli = Cli::parse();
    if let Some(Command::Check(args)) = cli.command.take() {
        return run_check(args);
    }
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("solx: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_check(args: CheckArgs) -> ExitCode {
    use solx::analysis::{AnalysisOptions, AnalysisReport, analyze_source};
    use solx::check::{MAX_SOURCE_BYTES, check_source, json_analysis_report, json_report};
    let source_name = args
        .file
        .as_ref()
        .map_or_else(|| "<stdin>".into(), |path| path.display().to_string());
    let read = || -> Result<String, String> {
        let reader: Box<dyn Read> = match &args.file {
            Some(path) => Box::new(
                std::fs::File::open(path)
                    .map_err(|error| format!("cannot open '{source_name}': {error}"))?,
            ),
            None => Box::new(io::stdin().lock()),
        };
        let mut bytes = Vec::new();
        reader
            .take(MAX_SOURCE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("cannot read '{source_name}': {error}"))?;
        if bytes.len() > MAX_SOURCE_BYTES {
            return Err(format!(
                "check input exceeds {MAX_SOURCE_BYTES} UTF-8 bytes"
            ));
        }
        String::from_utf8(bytes).map_err(|_| "check input must be valid UTF-8".into())
    };
    let source = match read() {
        Ok(source) => source,
        Err(error) => {
            if args.json {
                println!(
                    "{}",
                    json_analysis_report(
                        &source_name,
                        &[],
                        Some(&error),
                        args.symbols.then_some(&[])
                    )
                );
            } else {
                eprintln!("solx: {error}");
            }
            return ExitCode::from(2);
        }
    };
    if args.names || args.symbols {
        let report = match analyze_source(
            &source,
            AnalysisOptions {
                names: args.names,
                symbols: args.symbols,
            },
        ) {
            Ok(report) => report,
            Err(error) => AnalysisReport {
                diagnostics: vec![error],
                symbols: Vec::new(),
            },
        };
        if args.json {
            println!(
                "{}",
                json_analysis_report(
                    &source_name,
                    &report.diagnostics,
                    None,
                    args.symbols.then_some(report.symbols.as_slice())
                )
            );
        } else if report.diagnostics.is_empty() {
            println!("No syntax or name errors in {source_name}");
        } else {
            for diagnostic in &report.diagnostics {
                eprintln!(
                    "solx: {} at {source_name}:{}:{}",
                    diagnostic.message, diagnostic.span.line, diagnostic.span.column
                );
            }
        }
        return if report.diagnostics.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    match check_source(&source) {
        Ok(()) => {
            if args.json {
                println!("{}", json_report(&source_name, None, None));
            } else {
                println!("No syntax errors in {source_name}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            if args.json {
                println!("{}", json_report(&source_name, Some(&error), None));
            } else {
                eprintln!(
                    "solx: {} at {source_name}:{}:{}",
                    error.message, error.span.line, error.span.column
                );
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let mut interpreter = Interpreter::with_limits(ExecutionLimits {
        max_steps: if cli.unlimited_steps {
            None
        } else {
            Some(cli.max_steps)
        },
        max_call_depth: cli.max_call_depth,
        max_import_depth: cli.max_import_depth,
        max_duration: cli.timeout_ms.map(Duration::from_millis),
        max_source_bytes: cli.max_source_bytes,
        max_format_bytes: cli.max_format_bytes,
        max_format_work: cli.max_format_work,
        ..ExecutionLimits::default()
    })
    .map_err(|error| error.to_string())?;
    let mut capabilities = HostCapabilities::default();
    if cli.allow_read || cli.allow_write {
        capabilities = capabilities
            .with_filesystem(
                cli.fs_root
                    .as_deref()
                    .unwrap_or_else(|| std::path::Path::new(".")),
                cli.allow_read,
                cli.allow_write,
            )
            .map_err(|error| format!("cannot open filesystem root: {error}"))?;
    }
    if cli.allow_time {
        capabilities = capabilities.with_time();
    }
    interpreter.set_capabilities(capabilities);
    let cancellation = interpreter.cancellation_token();
    ctrlc::set_handler(move || cancellation.cancel())
        .map_err(|error| format!("cannot install Ctrl+C handler: {error}"))?;
    #[cfg(windows)]
    {
        // A launcher using CREATE_NEW_PROCESS_GROUP can pass down an ignored
        // Ctrl+C flag. Registering a handler does not clear that inherited flag.
        // SAFETY: no pointers or callbacks are passed; this only enables Ctrl+C
        // delivery to this process after its handler has been installed.
        if unsafe { windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 0) } == 0 {
            return Err(format!(
                "cannot enable Ctrl+C: {}",
                io::Error::last_os_error()
            ));
        }
    }
    let mut executed = false;

    if let Some(source) = cli.execute {
        interpreter
            .run_source(&source)
            .map_err(|error| error.to_string())?;
        executed = true;
    }

    if let Some(path) = cli.file {
        interpreter
            .run_file(&path)
            .map_err(|error| error.to_string())?;

        executed = true;
    }

    if !executed || cli.interactive {
        repl(interpreter)?;
    }

    Ok(())
}

fn repl(mut interpreter: Interpreter) -> Result<(), String> {
    let cancellation = interpreter.cancellation_token();
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut source = String::new();
    let max_source_bytes = interpreter.limits().max_source_bytes;

    println!("Solx {} interactive mode", env!("CARGO_PKG_VERSION"));
    println!("Type exit or quit to leave; Ctrl+C cancels running code.");

    loop {
        print!("{}", if source.is_empty() { "> " } else { "... " });

        io::stdout()
            .flush()
            .map_err(|error| format!("failed to write prompt: {error}"))?;

        let input = match read_repl_line(&mut reader, max_source_bytes.saturating_sub(source.len()))
        {
            Ok(input) => input,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                cancellation.reset();
                source.clear();
                continue;
            }
            Err(error) => return Err(format!("failed to read input: {error}")),
        };

        // Some consoles finish the pending read only after Enter. Discard the
        // cancelled input rather than executing a partial line on that platform.
        if cancellation.is_cancelled() {
            cancellation.reset();
            source.clear();
            eprintln!("Input cancelled.");
            continue;
        }

        let line = match input {
            ReplLine::TooLong => {
                eprintln!("Error: source byte limit exceeded ({max_source_bytes})");
                source.clear();
                continue;
            }
            ReplLine::Eof => {
                if !source.trim().is_empty() {
                    eprintln!("Error: incomplete input at end of input");
                }
                println!();
                break;
            }
            ReplLine::Source(line) => line,
        };

        if source.is_empty() {
            let trimmed = line.trim();

            if trimmed.is_empty() {
                continue;
            }

            if trimmed == "exit" || trimmed == "quit" {
                break;
            }
        }

        source.push_str(&line);

        match parse_with_limits(&source, interpreter.limits().parse) {
            Ok(program) => {
                if let Err(error) = interpreter.run_program(&program) {
                    eprintln!("Error: {error}");
                }
                cancellation.reset();
                source.clear();
            }
            Err(error) => {
                if needs_more_input(&source, &error) {
                    continue;
                }

                eprintln!("Error: {error}");
                source.clear();
            }
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum ReplLine {
    Eof,
    Source(String),
    TooLong,
}

fn read_repl_line(reader: &mut impl BufRead, maximum: usize) -> io::Result<ReplLine> {
    let mut bytes = Vec::new();
    let limit = u64::try_from(maximum).unwrap_or(u64::MAX).saturating_add(1);
    (&mut *reader).take(limit).read_until(b'\n', &mut bytes)?;
    if bytes.len() > maximum {
        // Discard the rest of this physical line without retaining its bytes.
        // A truncated suffix must never become the next executable REPL input.
        if bytes.last() != Some(&b'\n') {
            reader.skip_until(b'\n')?;
        }
        return Ok(ReplLine::TooLong);
    }
    if bytes.is_empty() {
        return Ok(ReplLine::Eof);
    }
    String::from_utf8(bytes)
        .map(ReplLine::Source)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn needs_more_input(source: &str, error: &ParseError) -> bool {
    // Resource limits cannot be fixed by appending another line, even at EOF.
    if error.limit.is_some() {
        return false;
    }
    if error.message == "unterminated raw string literal" {
        return true;
    }
    let Ok(tokens) = tokenize(source) else {
        return false;
    };

    // Only a parse error at EOF can be resolved by appending more input.
    // Runtime error text must never cause already executed code to be retried.
    tokens
        .last()
        .is_some_and(|token| token.kind == TokenKind::Eof && token.span == error.span)
}

#[cfg(test)]
mod input_tests {
    use super::*;

    #[test]
    fn bounded_repl_input_discards_the_entire_oversized_line() {
        let input = format!(
            "{}print('must not execute')\nprint('next')\n",
            "x".repeat(1000)
        );
        let mut reader = io::Cursor::new(input.into_bytes());
        assert_eq!(read_repl_line(&mut reader, 20).unwrap(), ReplLine::TooLong);
        assert_eq!(
            read_repl_line(&mut reader, 20).unwrap(),
            ReplLine::Source("print('next')\n".into())
        );
        assert_eq!(read_repl_line(&mut reader, 20).unwrap(), ReplLine::Eof);
    }

    #[test]
    fn bounded_repl_input_checks_utf8_bytes_before_decoding_a_truncated_character() {
        let mut reader = io::Cursor::new("\u{e9}\nnext\n".as_bytes());
        assert_eq!(read_repl_line(&mut reader, 1).unwrap(), ReplLine::TooLong);
        assert_eq!(
            read_repl_line(&mut reader, 5).unwrap(),
            ReplLine::Source("next\n".into())
        );
        let mut exact = io::Cursor::new("\u{e9}\n".as_bytes());
        assert_eq!(
            read_repl_line(&mut exact, 3).unwrap(),
            ReplLine::Source("\u{e9}\n".into())
        );
        assert_eq!(read_repl_line(&mut exact, 0).unwrap(), ReplLine::Eof);
    }
}
