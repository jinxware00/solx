//! Conservative, single-buffer name analysis and declaration symbols.
//! This traverses the real AST and lexer tokens; it never loads or executes modules.

use std::collections::HashSet;

use crate::check::{CheckDiagnostic, parse_for_analysis};
use crate::lexer::{Span, Token, TokenKind};
use crate::parser::ast::{DeclarationKind, Expr, Function, Stmt};

pub const MAX_NAME_DIAGNOSTICS: usize = 100;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisOptions {
    pub names: bool,
    pub symbols: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Variable,
    Constant,
    Module,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Variable => "variable",
            Self::Constant => "constant",
            Self::Module => "module",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationSymbol {
    pub name: String,
    pub kind: SymbolKind,
    /// Full parser-produced declaration statement range.
    pub range: Span,
    /// Exact name token; implicit imports use their string-specifier token.
    pub selection_range: Span,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AnalysisReport {
    pub diagnostics: Vec<CheckDiagnostic>,
    pub symbols: Vec<DeclarationSymbol>,
}

/// Parse a buffer and optionally analyze names and gather declaration symbols.
/// Syntax failures stop analysis. File globals and each block's immediate locals
/// are predeclared, deliberately allowing captures of later declarations.
pub fn analyze_source(
    source: &str,
    options: AnalysisOptions,
) -> Result<AnalysisReport, CheckDiagnostic> {
    let (program, tokens) = parse_for_analysis(source)?;
    let mut globals = HashSet::new();
    collect_globals(&program.statements, &mut globals);
    let mut analyzer = Analyzer {
        tokens: &tokens,
        cursor: 0,
        options,
        globals,
        scopes: Vec::new(),
        report: AnalysisReport::default(),
    };
    // A standalone buffer may be an imported module. Without loading its callers
    // we conservatively admit the module's implicit exports binding.
    analyzer.body(&program.statements, &["print", "str", "num", "exports"]);
    Ok(analyzer.report)
}

struct Analyzer<'a> {
    tokens: &'a [Token],
    cursor: usize,
    options: AnalysisOptions,
    globals: HashSet<String>,
    scopes: Vec<HashSet<String>>,
    report: AnalysisReport,
}

impl Analyzer<'_> {
    fn body(&mut self, statements: &[Stmt], bindings: &[&str]) {
        let mut scope: HashSet<String> = bindings.iter().map(|name| (*name).to_owned()).collect();
        for statement in statements {
            match statement.unlocated() {
                Stmt::Import { name, .. } | Stmt::Function { name, .. } => {
                    scope.insert(name.clone());
                }
                Stmt::Declaration {
                    kind: DeclarationKind::Var | DeclarationKind::Const,
                    name,
                    ..
                } => {
                    scope.insert(name.clone());
                }
                _ => {}
            }
        }
        self.scopes.push(scope);
        for statement in statements {
            self.statement(statement);
        }
        self.scopes.pop();
    }

    fn statement(&mut self, statement: &Stmt) {
        let range = match statement {
            Stmt::Located { span, .. } => Some(*span),
            _ => None,
        };
        match statement.unlocated() {
            Stmt::Located { .. } => unreachable!("unlocated removes all wrappers"),
            Stmt::Import { specifier, name } => {
                let literal = self.string(specifier);
                let selection = if matches!(self.tokens.get(self.cursor).map(|token| &token.kind), Some(TokenKind::Identifier(word)) if word == "as")
                {
                    self.identifier("as");
                    self.identifier(name)
                } else {
                    literal
                };
                self.symbol(name, SymbolKind::Module, range, selection);
            }
            Stmt::Function { name, function } => {
                let selection = self.identifier(name);
                self.symbol(name, SymbolKind::Function, range, selection);
                self.function(function);
            }
            Stmt::Declaration {
                kind,
                name,
                type_annotation,
                initializer,
            } => {
                let selection = self.identifier(name);
                let kind = if matches!(kind, DeclarationKind::Const) {
                    SymbolKind::Constant
                } else {
                    SymbolKind::Variable
                };
                self.symbol(name, kind, range, selection);
                if let Some(annotation) = type_annotation {
                    self.identifier(annotation);
                }
                if let Some(initializer) = initializer {
                    self.expression(initializer);
                }
            }
            Stmt::Assignment { target, value } => {
                self.expression(target);
                self.expression(value);
            }
            Stmt::Expression(expression) => self.expression(expression),
            Stmt::Return(values) => {
                for value in values {
                    self.expression(value);
                }
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::TryCatch {
                body,
                error_name,
                catch_body,
            } => {
                self.body(body, &[]);
                self.identifier(error_name);
                self.body(catch_body, &[error_name]);
            }
            Stmt::While { condition, body } => {
                self.expression(condition);
                self.body(body, &[]);
            }
            Stmt::NumericFor {
                name,
                start,
                limit,
                step,
                body,
            } => {
                self.identifier(name);
                self.expression(start);
                self.expression(limit);
                if let Some(step) = step {
                    self.expression(step);
                }
                self.body(body, &[name]);
            }
            Stmt::CollectionFor {
                name,
                second_name,
                iterable,
                body,
            } => {
                self.identifier(name);
                if let Some(second) = second_name {
                    self.identifier(second);
                }
                self.expression(iterable);
                let mut bindings = vec![name.as_str()];
                if let Some(second) = second_name {
                    bindings.push(second);
                }
                self.body(body, &bindings);
            }
            Stmt::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    self.expression(condition);
                    self.body(body, &[]);
                }
                if let Some(body) = else_branch {
                    self.body(body, &[]);
                }
            }
        }
    }

    fn function(&mut self, function: &Function) {
        for parameter in &function.parameters {
            self.identifier(parameter);
        }
        let parameters: Vec<_> = function.parameters.iter().map(String::as_str).collect();
        self.body(&function.body, &parameters);
    }

    fn expression(&mut self, expression: &Expr) {
        match expression {
            Expr::Identifier(name) => {
                let span = self.identifier(name);
                if self.options.names
                    && self.report.diagnostics.len() < MAX_NAME_DIAGNOSTICS
                    && !self.globals.contains(name)
                    && !self.scopes.iter().rev().any(|scope| scope.contains(name))
                    && let Some(span) = span
                {
                    self.report.diagnostics.push(CheckDiagnostic {
                        code: "undefined-name",
                        message: format!(
                            "name '{name}' is not declared in this file or lexical scope"
                        ),
                        span,
                    });
                }
            }
            Expr::String(value) => {
                self.string(value);
            }
            Expr::Function(function) => self.function(function),
            Expr::Nil | Expr::Boolean(_) | Expr::Number(_) => {}
            Expr::Unary { operand, .. } => self.expression(operand),
            Expr::Binary { left, right, .. } => {
                self.expression(left);
                self.expression(right);
            }
            Expr::Call { callee, arguments } => {
                self.expression(callee);
                for argument in arguments {
                    self.expression(argument);
                }
            }
            Expr::Index { object, index } => {
                self.expression(object);
                self.expression(index);
            }
            Expr::Property { object, name } => {
                self.expression(object);
                self.identifier(name);
            }
            Expr::List(items) => {
                for item in items {
                    self.expression(item);
                }
            }
            Expr::Object(properties) => {
                for (name, value) in properties {
                    self.named_token(name, true);
                    self.expression(value);
                }
            }
        }
    }

    fn symbol(
        &mut self,
        name: &str,
        kind: SymbolKind,
        range: Option<Span>,
        selection: Option<Span>,
    ) {
        if self.options.symbols
            && let (Some(range), Some(selection_range)) = (range, selection)
            && selection_range.start >= range.start
            && selection_range.end <= range.end
        {
            self.report.symbols.push(DeclarationSymbol {
                name: name.to_owned(),
                kind,
                range,
                selection_range,
            });
        }
    }

    fn identifier(&mut self, name: &str) -> Option<Span> {
        self.named_token(name, false)
    }

    fn string(&mut self, value: &str) -> Option<Span> {
        let token = self.next_named_token()?;
        let matches = matches!(&token.kind, TokenKind::String(actual) if actual == value);
        debug_assert!(matches, "AST/token string order mismatch: {token:?}");
        matches.then_some(token.span)
    }

    fn named_token(&mut self, name: &str, allow_string: bool) -> Option<Span> {
        let token = self.next_named_token()?;
        let matches = match &token.kind {
            TokenKind::Identifier(actual) => actual == name,
            TokenKind::String(actual) => allow_string && actual == name,
            _ => false,
        };
        debug_assert!(
            matches,
            "AST/token name order mismatch for {name}: {token:?}"
        );
        matches.then_some(token.span)
    }

    // AST traversal is in lexical order. Match only its name/string terminals;
    // punctuation, operators and keywords are already validated by the parser.
    // This supplies exact locations without implementing another expression parser.
    fn next_named_token(&mut self) -> Option<&Token> {
        while let Some(token) = self.tokens.get(self.cursor) {
            self.cursor += 1;
            if matches!(token.kind, TokenKind::Identifier(_) | TokenKind::String(_)) {
                return Some(token);
            }
        }
        None
    }
}

fn collect_globals(statements: &[Stmt], globals: &mut HashSet<String>) {
    for statement in statements {
        match statement.unlocated() {
            Stmt::Located { .. } => unreachable!("unlocated removes all wrappers"),
            Stmt::Declaration {
                kind,
                name,
                initializer,
                ..
            } => {
                if matches!(kind, DeclarationKind::Global) {
                    globals.insert(name.clone());
                }
                if let Some(value) = initializer {
                    expression_globals(value, globals);
                }
            }
            Stmt::Function { function, .. } => collect_globals(&function.body, globals),
            Stmt::Assignment { target, value } => {
                expression_globals(target, globals);
                expression_globals(value, globals);
            }
            Stmt::Expression(expression) => expression_globals(expression, globals),
            Stmt::Return(values) => {
                for value in values {
                    expression_globals(value, globals);
                }
            }
            Stmt::TryCatch {
                body, catch_body, ..
            } => {
                collect_globals(body, globals);
                collect_globals(catch_body, globals);
            }
            Stmt::While { condition, body } => {
                expression_globals(condition, globals);
                collect_globals(body, globals);
            }
            Stmt::NumericFor {
                start,
                limit,
                step,
                body,
                ..
            } => {
                expression_globals(start, globals);
                expression_globals(limit, globals);
                if let Some(step) = step {
                    expression_globals(step, globals);
                }
                collect_globals(body, globals);
            }
            Stmt::CollectionFor { iterable, body, .. } => {
                expression_globals(iterable, globals);
                collect_globals(body, globals);
            }
            Stmt::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    expression_globals(condition, globals);
                    collect_globals(body, globals);
                }
                if let Some(body) = else_branch {
                    collect_globals(body, globals);
                }
            }
            Stmt::Import { .. } | Stmt::Break | Stmt::Continue => {}
        }
    }
}

fn expression_globals(expression: &Expr, globals: &mut HashSet<String>) {
    match expression {
        Expr::Function(function) => collect_globals(&function.body, globals),
        Expr::Unary { operand, .. } => expression_globals(operand, globals),
        Expr::Binary { left, right, .. } => {
            expression_globals(left, globals);
            expression_globals(right, globals);
        }
        Expr::Call { callee, arguments } => {
            expression_globals(callee, globals);
            for argument in arguments {
                expression_globals(argument, globals);
            }
        }
        Expr::Index { object, index } => {
            expression_globals(object, globals);
            expression_globals(index, globals);
        }
        Expr::Property { object, .. } => expression_globals(object, globals),
        Expr::List(items) => {
            for item in items {
                expression_globals(item, globals);
            }
        }
        Expr::Object(properties) => {
            for (_, value) in properties {
                expression_globals(value, globals);
            }
        }
        Expr::Identifier(_) | Expr::Nil | Expr::Boolean(_) | Expr::Number(_) | Expr::String(_) => {}
    }
}
