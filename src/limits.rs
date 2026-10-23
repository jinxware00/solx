//! Cooperative execution limits for the tree-walking interpreter.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Conservative ceilings while parsing/evaluation still use the Rust stack.
pub const MAX_PARSE_NESTING: usize = 64;
pub const MAX_AST_DEPTH: usize = 128;
pub const MAX_EVALUATION_DEPTH: usize = 64;
pub const MAX_CALL_DEPTH: usize = 32;
pub const MAX_IMPORT_DEPTH: usize = 16;
pub const DEFAULT_MAX_STEPS: u64 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    ParseNesting,
    AstDepth,
    EvaluationDepth,
    CallDepth,
    ImportDepth,
    Steps,
}

impl LimitKind {
    pub(crate) fn message(self, limit: impl std::fmt::Display) -> String {
        let name = match self {
            Self::ParseNesting => "parser nesting",
            Self::AstDepth => "AST depth",
            Self::EvaluationDepth => "evaluation depth",
            Self::CallDepth => "function call depth",
            Self::ImportDepth => "import depth",
            Self::Steps => "execution step",
        };
        format!("{name} limit exceeded ({limit})")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseLimits {
    /// Combined active statement/expression parser entries.
    pub max_nesting: usize,
    /// AST nodes along a path, including Located statement wrappers.
    pub max_ast_depth: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_nesting: MAX_PARSE_NESTING,
            max_ast_depth: MAX_AST_DEPTH,
        }
    }
}

impl ParseLimits {
    pub(crate) fn validate(self) -> Result<(), String> {
        check("max_nesting", self.max_nesting, MAX_PARSE_NESTING)?;
        check("max_ast_depth", self.max_ast_depth, MAX_AST_DEPTH)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionLimits {
    pub parse: ParseLimits,
    /// Combined active statement/expression evaluations, including imports/calls.
    pub max_evaluation_depth: usize,
    pub max_call_depth: usize,
    /// Simultaneous uncached imports; the entry file does not count.
    pub max_import_depth: usize,
    /// Per public run, shared with imports/calls. None disables only this budget.
    pub max_steps: Option<u64>,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            parse: ParseLimits::default(),
            max_evaluation_depth: MAX_EVALUATION_DEPTH,
            max_call_depth: MAX_CALL_DEPTH,
            max_import_depth: MAX_IMPORT_DEPTH,
            max_steps: Some(DEFAULT_MAX_STEPS),
        }
    }
}

impl ExecutionLimits {
    pub(crate) fn validate(self) -> Result<(), String> {
        self.parse.validate()?;
        check(
            "max_evaluation_depth",
            self.max_evaluation_depth,
            MAX_EVALUATION_DEPTH,
        )?;
        check("max_call_depth", self.max_call_depth, MAX_CALL_DEPTH)?;
        check("max_import_depth", self.max_import_depth, MAX_IMPORT_DEPTH)
    }
}

fn check(name: &str, value: usize, maximum: usize) -> Result<(), String> {
    if (1..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} must be between 1 and {maximum}"))
    }
}

/// A thread-safe request flag; the interpreter itself remains single-threaded.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Clear a request only after the cancelled run and its controller have ended.
    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}
