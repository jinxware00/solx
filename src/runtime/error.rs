use std::fmt;
use std::path::{Path, PathBuf};

use super::Value;
use crate::lexer::Span;
use crate::limits::LimitKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorKind {
    Language,
    LimitExceeded(LimitKind),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: Option<PathBuf>,
    pub span: Option<Span>,
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(path) => write!(f, "{}", path.display())?,
            None => write!(f, "<source>")?,
        }
        if let Some(span) = self.span {
            write!(f, ":{}:{}", span.line, span.column)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorContext {
    pub description: String,
    pub location: Option<SourceLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub kind: RuntimeErrorKind,
    /// Original message, without source prefixes or call-stack decoration.
    pub message: String,
    /// Keep cold diagnostic data off recursive Result stack frames.
    pub location: Option<Box<SourceLocation>>,
    /// Innermost call/import first.
    pub contexts: Vec<ErrorContext>,
}

impl RuntimeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            kind: RuntimeErrorKind::Language,
            message: message.into(),
            location: None,
            contexts: Vec::new(),
        }
    }

    pub(crate) fn limit(kind: LimitKind, maximum: impl fmt::Display) -> Self {
        Self {
            kind: RuntimeErrorKind::LimitExceeded(kind),
            ..Self::new(kind.message(maximum))
        }
    }

    pub(crate) fn cancelled() -> Self {
        Self {
            kind: RuntimeErrorKind::Cancelled,
            ..Self::new("execution cancelled")
        }
    }

    pub fn is_catchable(&self) -> bool {
        self.kind == RuntimeErrorKind::Language
    }

    /// A detached snapshot for a Solx catch binding; no live stack or scope links.
    pub(crate) fn to_value(&self) -> Value {
        fn location_value(location: Option<&SourceLocation>) -> Value {
            let Some(location) = location else {
                return Value::Nil;
            };
            Value::object(
                [
                    (
                        "file".into(),
                        Value::String(
                            location.file.as_ref().map_or_else(
                                || "<source>".into(),
                                |file| file.display().to_string(),
                            ),
                        ),
                    ),
                    (
                        "line".into(),
                        location
                            .span
                            .map_or(Value::Nil, |span| Value::Number(span.line as f64)),
                    ),
                    (
                        "column".into(),
                        location
                            .span
                            .map_or(Value::Nil, |span| Value::Number(span.column as f64)),
                    ),
                ]
                .into(),
            )
        }

        Value::object(
            [
                ("message".into(), Value::String(self.message.clone())),
                ("location".into(), location_value(self.location.as_deref())),
                (
                    "trace".into(),
                    Value::list(
                        self.contexts
                            .iter()
                            .map(|context| {
                                Value::object(
                                    [
                                        (
                                            "description".into(),
                                            Value::String(context.description.clone()),
                                        ),
                                        (
                                            "location".into(),
                                            location_value(context.location.as_ref()),
                                        ),
                                    ]
                                    .into(),
                                )
                            })
                            .collect(),
                    ),
                ),
            ]
            .into(),
        )
    }

    pub(crate) fn at(mut self, file: Option<&Path>, span: Option<Span>) -> Self {
        if self.location.is_none() {
            self.location = Some(Box::new(SourceLocation {
                file: file.map(Path::to_owned),
                span,
            }));
        }
        self
    }

    pub(crate) fn context(
        mut self,
        description: impl Into<String>,
        location: Option<SourceLocation>,
    ) -> Self {
        self.contexts.push(ErrorContext {
            description: description.into(),
            location,
        });
        self
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(location) = &self.location {
            write!(f, " at {location}")?;
        }
        for context in &self.contexts {
            write!(f, "\n  {}", context.description)?;
            if let Some(location) = &context.location {
                write!(f, " at {location}")?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeError {}
