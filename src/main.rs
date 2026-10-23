use clap::Parser;
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    process::ExitCode,
};

use solx::{
    interpreter::Interpreter,
    lexer::{TokenKind, tokenize},
    limits::{DEFAULT_MAX_STEPS, ExecutionLimits, MAX_CALL_DEPTH, MAX_IMPORT_DEPTH},
    parser::{ParseError, parse_with_limits},
};

#[derive(Debug, Parser)]
#[command(
    name = "solx",
    version,
    about = "The Solx programming language",
    long_about = "Solx: a programming language focused on performance and low memory usage."
)]
struct Cli {
    /// Execute source code passed directly on the command line
    #[arg(short = 'e', long = "execute", value_name = "CODE")]
    execute: Option<String>,

    /// Enter interactive mode after executing code or a file
    #[arg(short = 'i', long = "interactive")]
    interactive: bool,

    /// Maximum evaluation steps per file, -e command, or REPL buffer
    #[arg(long, value_name = "N", default_value_t = DEFAULT_MAX_STEPS)]
    max_steps: u64,

    /// Disable the step budget; depth limits and Ctrl+C still apply
    #[arg(long, conflicts_with = "max_steps")]
    unlimited_steps: bool,

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

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("solx: {error}");
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
        ..ExecutionLimits::default()
    })
    .map_err(|error| error.to_string())?;
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
    let mut line = String::new();
    let mut source = String::new();

    println!("Solx {} interactive mode", env!("CARGO_PKG_VERSION"));
    println!("Type exit or quit to leave; Ctrl+C cancels running code.");

    loop {
        print!("{}", if source.is_empty() { "> " } else { "... " });

        io::stdout()
            .flush()
            .map_err(|error| format!("failed to write prompt: {error}"))?;

        line.clear();

        let bytes = match reader.read_line(&mut line) {
            Ok(bytes) => bytes,
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

        if bytes == 0 {
            if !source.trim().is_empty() {
                eprintln!("Error: incomplete input at end of input");
            }
            println!();
            break;
        }

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

fn needs_more_input(source: &str, error: &ParseError) -> bool {
    // Resource limits cannot be fixed by appending another line, even at EOF.
    if error.limit.is_some() {
        return false;
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
