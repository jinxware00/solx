//! Non-executing lexer/parser checks. No interpreter or module loader is created.

use crate::analysis::DeclarationSymbol;
use crate::lexer::{Span, Token, tokenize};
use crate::parser::{ParseError, ast::Program, parse_tokens};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SOURCE_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

/// Returns the first real lexer/parser failure, including syntax resource limits.
/// This checks only this buffer: it does not resolve imports or infer types.
pub fn check_source(source: &str) -> Result<(), CheckDiagnostic> {
    let tokens = checked_tokens(source)?;
    parse_tokens(tokens).map_err(parse_diagnostic)?;
    Ok(())
}

fn checked_tokens(source: &str) -> Result<Vec<Token>, CheckDiagnostic> {
    tokenize(source).map_err(|error| CheckDiagnostic {
        code: "lex-error",
        message: error.message,
        span: error.span,
    })
}

fn parse_diagnostic(error: ParseError) -> CheckDiagnostic {
    CheckDiagnostic {
        code: if error.limit.is_some() {
            "syntax-limit"
        } else {
            "parse-error"
        },
        message: error.message,
        span: error.span,
    }
}

pub(crate) fn parse_for_analysis(source: &str) -> Result<(Program, Vec<Token>), CheckDiagnostic> {
    let tokens = checked_tokens(source)?;
    let program = parse_tokens(tokens.clone()).map_err(parse_diagnostic)?;
    Ok((program, tokens))
}

/// Schema v1: half-open UTF-8 byte offsets into the exact input buffer.
pub fn json_report(
    source_name: &str,
    diagnostic: Option<&CheckDiagnostic>,
    input_error: Option<&str>,
) -> serde_json::Value {
    json_analysis_report(
        source_name,
        diagnostic.map_or(&[], std::slice::from_ref),
        input_error,
        None,
    )
}

/// Extends the existing check schema with multiple name diagnostics and optional
/// flat declaration symbols. All offsets remain half-open UTF-8 byte ranges.
pub fn json_analysis_report(
    source_name: &str,
    diagnostics: &[CheckDiagnostic],
    input_error: Option<&str>,
    symbols: Option<&[DeclarationSymbol]>,
) -> serde_json::Value {
    let diagnostics: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| {
            serde_json::json!({
                "code": diagnostic.code,
                "severity": "error",
                "message": diagnostic.message,
                "range": { "start": diagnostic.span.start, "end": diagnostic.span.end },
            })
        })
        .collect();
    let mut report = serde_json::json!({
        "schemaVersion": SCHEMA_VERSION,
        "offsetEncoding": "utf-8",
        "source": source_name,
        "ok": diagnostics.is_empty() && input_error.is_none(),
        "diagnostics": diagnostics,
        "error": input_error.map(|message| serde_json::json!({ "code": "input-error", "message": message })),
    });
    if let Some(symbols) = symbols {
        report["symbols"] = symbols.iter().map(|symbol| serde_json::json!({
            "name": symbol.name,
            "kind": symbol.kind.as_str(),
            "range": { "start": symbol.range.start, "end": symbol.range.end },
            "selectionRange": { "start": symbol.selection_range.start, "end": symbol.selection_range.end },
        })).collect();
    }
    report
}
