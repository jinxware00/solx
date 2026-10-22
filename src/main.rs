use clap::Parser;
use std::{
    fs,
    io::{self, BufRead, Write},
    path::PathBuf,
    process::ExitCode,
};

use solx::{
    interpreter::Interpreter,
    lexer::{TokenKind, tokenize},
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
    let mut interpreter = Interpreter::new();
    let mut executed = false;

    if let Some(source) = cli.execute {
        interpreter
            .run_source(&source)
            .map_err(|error| error.to_string())?;
        executed = true;
    }

    if let Some(path) = cli.file {
        let source = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read '{}': {error}", path.display()))?;

        interpreter
            .run_source(&source)
            .map_err(|error| format!("{}: {error}", path.display()))?;

        executed = true;
    }

    if !executed || cli.interactive {
        repl(interpreter)?;
    }

    Ok(())
}

fn repl(mut interpreter: Interpreter) -> Result<(), String> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();
    let mut source = String::new();

    println!("Solx {} interactive mode", env!("CARGO_PKG_VERSION"));
    println!("Type exit or quit to leave.");

    loop {
        print!("{}", if source.is_empty() { "> " } else { "... " });

        io::stdout()
            .flush()
            .map_err(|error| format!("failed to write prompt: {error}"))?;

        line.clear();

        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| format!("failed to read input: {error}"))?;

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

        match interpreter.run_source(source.trim()) {
            Ok(()) => source.clear(),
            Err(error) => {
                let message = error.to_string();

                if needs_more_input(&source, &message) {
                    continue;
                }

                eprintln!("Error: {message}");
                source.clear();
            }
        }
    }

    Ok(())
}

fn needs_more_input(source: &str, error: &str) -> bool {
    let Ok(tokens) = tokenize(source) else {
        return false;
    };

    let mut parentheses = 0_i32;
    let mut brackets = 0_i32;
    let mut braces = 0_i32;
    let mut open_blocks = 0_i32;

    for token in &tokens {
        match &token.kind {
            TokenKind::LeftParen => parentheses += 1,
            TokenKind::RightParen => parentheses -= 1,
            TokenKind::LeftBracket => brackets += 1,
            TokenKind::RightBracket => brackets -= 1,
            TokenKind::LeftBrace => braces += 1,
            TokenKind::RightBrace => braces -= 1,
            TokenKind::If | TokenKind::Fn => open_blocks += 1,
            TokenKind::End if open_blocks > 0 => open_blocks -= 1,
            _ => {}
        }
    }

    if parentheses > 0 || brackets > 0 || braces > 0 || open_blocks > 0 {
        return true;
    }

    error.to_ascii_lowercase().contains("found eof")
}
