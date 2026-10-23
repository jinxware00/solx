//! Non-executing lexer/parser checks. No interpreter or module loader is created.

use crate::lexer::{Span, tokenize};
use crate::parser::parse_tokens;

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
    let tokens = tokenize(source).map_err(|error| CheckDiagnostic {
        code: "lex-error",
        message: error.message,
        span: error.span,
    })?;
    parse_tokens(tokens).map_err(|error| CheckDiagnostic {
        code: if error.limit.is_some() {
            "syntax-limit"
        } else {
            "parse-error"
        },
        message: error.message,
        span: error.span,
    })?;
    Ok(())
}

/// Schema v1: half-open UTF-8 byte offsets into the exact input buffer.
pub fn json_report(
    source_name: &str,
    diagnostic: Option<&CheckDiagnostic>,
    input_error: Option<&str>,
) -> serde_json::Value {
    let diagnostics: Vec<_> = diagnostic
        .into_iter()
        .map(|diagnostic| {
            serde_json::json!({
                "code": diagnostic.code,
                "severity": "error",
                "message": diagnostic.message,
                "range": { "start": diagnostic.span.start, "end": diagnostic.span.end },
            })
        })
        .collect();
    serde_json::json!({
        "schemaVersion": SCHEMA_VERSION,
        "offsetEncoding": "utf-8",
        "source": source_name,
        "ok": diagnostics.is_empty() && input_error.is_none(),
        "diagnostics": diagnostics,
        "error": input_error.map(|message| serde_json::json!({ "code": "input-error", "message": message })),
    })
}
