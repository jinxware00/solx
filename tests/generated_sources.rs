//! Reproducible source generation through the real lexer/parser. Small cases
//! stay in process; long recursive/iterative syntax runs behind a CLI watchdog.

use std::io::{Read, Write};
use std::panic::catch_unwind;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use solx::check::check_source;
use solx::lexer::{Span, tokenize};
use solx::parser::parse;

const SEEDS: [u64; 4] = [1, 0x1234_5678_9abc_def0, 0xdead_beef, 0x5eed_2026_1023];

struct Generator(u64);

impl Generator {
    fn next(&mut self, choices: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % choices as u64) as usize
    }

    fn expression(&mut self, depth: usize) -> String {
        const LEAVES: &[&str] = &[
            "nil",
            "true",
            "false",
            "0",
            "42",
            ".5",
            "-17",
            "'é🙂'",
            "\"escaped\\nquote\\\"\"",
            "x",
            "9223372036854775807",
            "-9223372036854775808",
        ];
        if depth == 0 {
            return LEAVES[self.next(LEAVES.len())].into();
        }
        match self.next(9) {
            0 => format!("not ({})", self.expression(depth - 1)),
            1 => format!("-({})", self.expression(depth - 1)),
            2 => format!(
                "[{}, {}]",
                self.expression(depth - 1),
                self.expression(depth - 1)
            ),
            3 => format!(
                "{{left = {}, right = {}}}",
                self.expression(depth - 1),
                self.expression(depth - 1)
            ),
            4 => format!(
                "target({}, {})",
                self.expression(depth - 1),
                self.expression(depth - 1)
            ),
            5 => format!(
                "({})[{}]",
                self.expression(depth - 1),
                self.expression(depth - 1)
            ),
            6 => format!("(fn(a) return {} end)", self.expression(depth - 1)),
            7 => format!("({}).field", self.expression(depth - 1)),
            _ => {
                const OPERATORS: &[&str] =
                    &["+", "*", "^", "..", "and", "or", "==", "~=", "<", "//", "%"];
                let operator = OPERATORS[self.next(OPERATORS.len())];
                format!(
                    "({}) {operator} ({})",
                    self.expression(depth - 1),
                    self.expression(depth - 1)
                )
            }
        }
    }

    fn program(&mut self, index: usize) -> String {
        let expression = self.expression(3);
        let body = match self.next(7) {
            0 => format!("var value{index} = {expression}"),
            1 => format!("fn run(a) if a then return {expression} else return nil end end"),
            2 => {
                format!("for i = 1, 3 do var value = {expression} if i == 2 then continue end end")
            }
            3 => format!("try var value = {expression} catch err var message = err.message end"),
            4 => format!("import 'json' as j var value = {expression}"),
            5 => format!("while false do var value = {expression} break end"),
            _ => format!("const value = {expression}"),
        };
        format!("-- generated é🙂 case {index}\r\n{body}\n")
    }
}

fn span_is_in_source(span: Span, source: &str) {
    assert!(span.start <= span.end && span.end <= source.len());
    assert!(source.is_char_boundary(span.start) && source.is_char_boundary(span.end));
    assert!(span.line > 0 && span.column > 0);
}

fn inspect(source: &str, expected_valid: Option<bool>, label: &str) {
    assert!(
        source.len() < 32_768,
        "small-case generator exceeded its bound"
    );
    let result = catch_unwind(|| {
        match tokenize(source) {
            Ok(tokens) => {
                let mut previous_end = 0;
                for token in &tokens {
                    span_is_in_source(token.span, source);
                    assert!(token.span.start >= previous_end);
                    previous_end = token.span.end;
                }
                assert_eq!(tokens.last().unwrap().span.end, source.len());
            }
            Err(error) => span_is_in_source(error.span, source),
        }
        let parsed = parse(source);
        let checked = check_source(source);
        assert_eq!(parsed.is_ok(), checked.is_ok(), "parser/check disagreement");
        if let Some(expected) = expected_valid {
            assert_eq!(parsed.is_ok(), expected, "unexpected acceptance in {label}");
        }
        if let Err(error) = parsed {
            span_is_in_source(error.span, source);
            let diagnostic = checked.unwrap_err();
            assert_eq!(diagnostic.span, error.span);
            assert_eq!(diagnostic.message, error.message);
        }
    });
    assert!(result.is_ok(), "lexer/parser panicked in {label}");
}

#[test]
fn generated_valid_and_known_malformed_programs_follow_the_real_pipeline() {
    const FAULTS: &[&str] = &[
        "var =",
        ")",
        "import 42",
        "var bad = 1e+",
        "var bad = '\\q'",
        "@",
        "fn incomplete(",
        "try catch end",
        "if true then",
        "return 1",
        "continue",
        "var bad = [1,,2]",
    ];
    for seed in SEEDS {
        let mut generator = Generator(seed);
        for index in 0..96 {
            let source = generator.program(index);
            let label = format!("seed {seed:#x}, case {index}");
            inspect(&source, Some(true), &label);
            let fault = FAULTS[generator.next(FAULTS.len())];
            inspect(
                &format!("{source}\n{fault}"),
                Some(false),
                &format!("{label}, fault {fault:?}"),
            );
        }
    }
}

#[test]
fn deterministic_truncations_and_unicode_mutations_never_panic() {
    for seed in SEEDS {
        let mut generator = Generator(seed);
        for index in 0..64 {
            let source = generator.program(index);
            let boundaries: Vec<_> = source.char_indices().map(|(offset, _)| offset).collect();
            let offset = boundaries[generator.next(boundaries.len())];
            let label = format!("seed {seed:#x}, mutation {index}, byte {offset}");
            inspect(&source[..offset], None, &format!("{label}, truncation"));
            let replacement = ["@", "🙂", "'", "\0", "\n", "\\"][generator.next(6)];
            let next = offset + source[offset..].chars().next().unwrap().len_utf8();
            let mutated = format!("{}{replacement}{}", &source[..offset], &source[next..]);
            inspect(&mutated, None, &label);
        }
    }
}

fn check_in_child(source: String) -> serde_json::Value {
    const OUTPUT_LIMIT: u64 = 16_384;
    let mut child = Command::new(env!("CARGO_BIN_EXE_solx"))
        .args(["check", "--json", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    // Start the watchdog before waiting for any pipe operation. Reads are
    // bounded and run concurrently, so unexpected output cannot fill a pipe.
    let deadline = Instant::now() + Duration::from_secs(10);
    let writer = std::thread::spawn(move || stdin.write_all(source.as_bytes()));
    let stdout_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    });
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break (status, false);
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            break (child.wait().unwrap(), true);
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let written = writer.join().unwrap();
    let stdout = stdout_reader.join().unwrap();
    let stderr = stderr_reader.join().unwrap();
    assert!(!timed_out, "generated source check exceeded ten seconds");
    assert!(written.is_ok(), "child stopped reading generated source");
    assert!(stdout.len() as u64 <= OUTPUT_LIMIT && stderr.len() as u64 <= OUTPUT_LIMIT);
    assert_eq!(
        status.code(),
        Some(1),
        "checker aborted or accepted deep input"
    );
    assert!(stderr.is_empty(), "checker wrote unexpected stderr");
    serde_json::from_slice(&stdout).unwrap()
}

#[test]
fn generated_deep_syntax_is_rejected_in_watched_subprocesses() {
    for seed in SEEDS {
        let mut generator = Generator(seed);
        let mut opening = String::from("var value = ");
        let mut closing = Vec::new();
        for _ in 0..4096 {
            let (left, right) = [("(", ")"), ("[", "]"), ("{x=", "}")][generator.next(3)];
            opening.push_str(left);
            closing.push(right);
        }
        let source = format!(
            "{opening}0{}",
            closing.into_iter().rev().collect::<String>()
        );
        let report = check_in_child(source);
        assert_eq!(report["ok"], false);
        assert_eq!(report["diagnostics"][0]["code"], "syntax-limit");
    }
    for tail in ["+1", ".field", "[1]", "()"] {
        let report = check_in_child(format!("var value = base{}", tail.repeat(8192)));
        assert_eq!(report["ok"], false);
        assert_eq!(report["diagnostics"][0]["code"], "syntax-limit");
    }
}
