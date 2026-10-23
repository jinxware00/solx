use std::fmt;

/// Source location for a token or lexical error.
/// Offsets are UTF-8 byte offsets; line and column are 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Number(String),
    String(String),

    And,
    Break,
    Catch,
    Const,
    Continue,
    Do,
    Else,
    ElseIf,
    End,
    False,
    Fn,
    For,
    Global,
    If,
    Import,
    In,
    Nil,
    Not,
    Or,
    Return,
    Then,
    True,
    Try,
    Var,
    While,

    Plus,
    Minus,
    Star,
    Slash,
    FloorSlash,
    Percent,
    Caret,
    Hash,
    Dot,
    Concat,
    Comma,
    Colon,
    Semicolon,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Assign,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,

    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub column: usize,
    /// Half-open UTF-8 range of the offending token, when produced by the lexer.
    pub span: Span,
}

impl LexError {
    pub fn new(message: impl Into<String>, line: usize, column: usize) -> Self {
        Self::at(
            message,
            Span {
                start: 0,
                end: 0,
                line,
                column,
            },
        )
    }

    pub fn at(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            line: span.line,
            column: span.column,
            span,
        }
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}:{}", self.message, self.line, self.column)
    }
}

impl std::error::Error for LexError {}
