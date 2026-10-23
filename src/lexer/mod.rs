//! Solx lexer.
//!
//! This module converts source text into tokens while preserving source
//! locations. It deliberately has no external dependencies.

mod token;

pub use token::{LexError, Span, Token, TokenKind};

pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).scan_tokens()
}

struct Lexer {
    chars: Vec<char>,
    current: usize,
    byte_offset: usize,
    line: usize,
    column: usize,
    token_start: usize,
    token_line: usize,
    token_column: usize,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            current: 0,
            byte_offset: 0,
            line: 1,
            column: 1,
            token_start: 0,
            token_line: 1,
            token_column: 1,
        }
    }

    fn scan_tokens(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace_and_comments();
            if self.is_at_end() {
                break;
            }

            self.token_start = self.byte_offset;
            self.token_line = self.line;
            self.token_column = self.column;

            let ch = self.advance().expect("not at end");
            let kind = match ch {
                '(' => TokenKind::LeftParen,
                ')' => TokenKind::RightParen,
                '{' => TokenKind::LeftBrace,
                '}' => TokenKind::RightBrace,
                '[' => TokenKind::LeftBracket,
                ']' => TokenKind::RightBracket,
                ',' => TokenKind::Comma,
                ':' => TokenKind::Colon,
                ';' => TokenKind::Semicolon,
                '+' => TokenKind::Plus,
                '-' => TokenKind::Minus,
                '*' => TokenKind::Star,
                '%' => TokenKind::Percent,
                '^' => TokenKind::Caret,
                '#' => TokenKind::Hash,
                '/' => {
                    if self.matches('/') {
                        TokenKind::FloorSlash
                    } else {
                        TokenKind::Slash
                    }
                }
                '.' => {
                    if self.matches('.') {
                        TokenKind::Concat
                    } else if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        // Support leading-dot decimals such as .5.
                        self.scan_number_after_leading_dot()?
                    } else {
                        TokenKind::Dot
                    }
                }
                '=' => {
                    if self.matches('=') {
                        TokenKind::Equal
                    } else {
                        TokenKind::Assign
                    }
                }
                '~' => {
                    if self.matches('=') {
                        TokenKind::NotEqual
                    } else {
                        return Err(self.error("expected '=' after '~'"));
                    }
                }
                '<' => {
                    if self.matches('=') {
                        TokenKind::LessEqual
                    } else {
                        TokenKind::Less
                    }
                }
                '>' => {
                    if self.matches('=') {
                        TokenKind::GreaterEqual
                    } else {
                        TokenKind::Greater
                    }
                }
                '"' if self.peek() == Some('"') && self.peek_next() == Some('"') => {
                    self.advance();
                    self.advance();
                    TokenKind::String(self.scan_raw_string()?)
                }
                '"' | '\'' => TokenKind::String(self.scan_string(ch)?),
                c if c.is_ascii_digit() => TokenKind::Number(self.scan_number(c)?),
                c if is_identifier_start(c) => {
                    let identifier = self.scan_identifier(c);
                    keyword_or_identifier(&identifier)
                }
                c if c.is_whitespace() => continue,
                other => {
                    return Err(self.error(format!("unexpected character '{other}'")));
                }
            };

            tokens.push(Token::new(kind, self.current_span()));
        }

        let eof_span = Span {
            start: self.byte_offset,
            end: self.byte_offset,
            line: self.line,
            column: self.column,
        };
        tokens.push(Token::new(TokenKind::Eof, eof_span));
        Ok(tokens)
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.advance();
            }

            if self.peek() == Some('-') && self.peek_next() == Some('-') {
                while self.peek().is_some_and(|c| c != '\n' && c != '\r') {
                    self.advance();
                }
            } else {
                break;
            }
        }
    }

    fn scan_identifier(&mut self, first: char) -> String {
        let mut text = String::from(first);
        while self.peek().is_some_and(is_identifier_continue) {
            text.push(self.advance().expect("peek confirmed a character"));
        }
        text
    }

    fn scan_number(&mut self, first: char) -> Result<String, LexError> {
        let mut text = String::from(first);

        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            text.push(self.advance().expect("peek confirmed a character"));
        }

        if self.peek() == Some('.') && self.peek_next().is_some_and(|c| c.is_ascii_digit()) {
            text.push(self.advance().expect("dot was present"));
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                text.push(self.advance().expect("peek confirmed a character"));
            }
        }

        self.scan_exponent(&mut text)?;
        Ok(text)
    }

    fn scan_number_after_leading_dot(&mut self) -> Result<TokenKind, LexError> {
        let mut text = String::from(".");
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            text.push(self.advance().expect("peek confirmed a character"));
        }
        self.scan_exponent(&mut text)?;
        Ok(TokenKind::Number(text))
    }

    fn scan_exponent(&mut self, text: &mut String) -> Result<(), LexError> {
        if !matches!(self.peek(), Some('e' | 'E')) {
            return Ok(());
        }

        text.push(self.advance().expect("exponent marker was present"));
        if matches!(self.peek(), Some('+' | '-')) {
            text.push(self.advance().expect("sign was present"));
        }

        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err(self.error("expected digit in numeric exponent"));
        }

        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            text.push(self.advance().expect("peek confirmed a character"));
        }
        Ok(())
    }

    fn scan_raw_string(&mut self) -> Result<String, LexError> {
        let mut value = String::new();
        while let Some(ch) = self.peek() {
            if ch == '"'
                && self.peek_next() == Some('"')
                && self.chars.get(self.current + 2) == Some(&'"')
            {
                self.advance();
                self.advance();
                self.advance();
                return Ok(value);
            }
            // advance preserves the existing UTF-8 offsets and line accounting;
            // raw content is copied verbatim, including CRLF and backslashes.
            value.push(self.advance().expect("peek confirmed a character"));
        }
        Err(self.error("unterminated raw string literal"))
    }

    fn scan_string(&mut self, quote: char) -> Result<String, LexError> {
        let mut value = String::new();

        while let Some(ch) = self.peek() {
            if ch == quote {
                self.advance();
                return Ok(value);
            }

            if ch == '\n' || ch == '\r' {
                return Err(self.error("newline in string literal"));
            }

            self.advance();
            if ch == '\\' {
                let escaped = self
                    .advance()
                    .ok_or_else(|| self.error("unterminated escape sequence"))?;
                match escaped {
                    'n' => value.push('\n'),
                    'r' => value.push('\r'),
                    't' => value.push('\t'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    '\'' => value.push('\''),
                    '{' => value.push('{'),
                    '}' => value.push('}'),
                    other => {
                        return Err(self.error(format!("unsupported escape sequence '\\{other}'")));
                    }
                }
            } else {
                value.push(ch);
            }
        }

        Err(self.error("unterminated string literal"))
    }

    fn matches(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.current).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.current + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.current += 1;
        self.byte_offset += ch.len_utf8();

        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }

        Some(ch)
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.chars.len()
    }

    fn current_span(&self) -> Span {
        Span {
            start: self.token_start,
            end: self.byte_offset,
            line: self.token_line,
            column: self.token_column,
        }
    }

    fn error(&self, message: impl Into<String>) -> LexError {
        LexError::at(message, self.current_span())
    }
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn keyword_or_identifier(text: &str) -> TokenKind {
    match text {
        "and" => TokenKind::And,
        "break" => TokenKind::Break,
        "catch" => TokenKind::Catch,
        "const" => TokenKind::Const,
        "continue" => TokenKind::Continue,
        "do" => TokenKind::Do,
        "else" => TokenKind::Else,
        "elseif" => TokenKind::ElseIf,
        "end" => TokenKind::End,
        "false" => TokenKind::False,
        "fn" => TokenKind::Fn,
        "for" => TokenKind::For,
        "global" => TokenKind::Global,
        "if" => TokenKind::If,
        "import" => TokenKind::Import,
        "in" => TokenKind::In,
        "nil" => TokenKind::Nil,
        "not" => TokenKind::Not,
        "or" => TokenKind::Or,
        "return" => TokenKind::Return,
        "then" => TokenKind::Then,
        "true" => TokenKind::True,
        "try" => TokenKind::Try,
        "var" => TokenKind::Var,
        "while" => TokenKind::While,
        _ => TokenKind::Identifier(text.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_keywords_and_identifiers() {
        let tokens = tokenize("var score = true\nfn getScore() return score end").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Var));
        assert!(matches!(&tokens[1].kind, TokenKind::Identifier(name) if name == "score"));
        assert!(matches!(tokens[3].kind, TokenKind::True));
        assert!(matches!(tokens[4].kind, TokenKind::Fn));
        assert!(matches!(tokens.last().unwrap().kind, TokenKind::Eof));
    }

    #[test]
    fn tokenizes_operators_longest_first() {
        let tokens = tokenize("a ~= b .. c // 2 <= 3").unwrap();
        let kinds: Vec<TokenKind> = tokens.into_iter().map(|token| token.kind).collect();
        assert!(matches!(kinds[1], TokenKind::NotEqual));
        assert!(matches!(kinds[3], TokenKind::Concat));
        assert!(matches!(kinds[5], TokenKind::FloorSlash));
        assert!(matches!(kinds[7], TokenKind::LessEqual));
    }

    #[test]
    fn handles_comments_and_source_positions() {
        let tokens = tokenize("-- comment\nvar x = 4").unwrap();
        assert!(matches!(tokens[0].kind, TokenKind::Var));
        assert_eq!(tokens[0].span.line, 2);
        assert_eq!(tokens[0].span.column, 1);
    }

    #[test]
    fn reads_escaped_unicode_strings() {
        let tokens = tokenize(r#""héllo\nworld""#).unwrap();
        assert!(matches!(&tokens[0].kind, TokenKind::String(value) if value == "héllo\nworld"));
    }

    #[test]
    fn reads_decimal_and_exponent_numbers() {
        let tokens = tokenize("12 3.5 .25 1e3 2.5e-2").unwrap();
        let values: Vec<String> = tokens
            .iter()
            .filter_map(|token| {
                if let TokenKind::Number(value) = &token.kind {
                    Some(value.clone())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(values, ["12", "3.5", ".25", "1e3", "2.5e-2"]);
    }

    #[test]
    fn reports_unterminated_strings() {
        let error = tokenize(r#""unfinished"#).unwrap_err();
        assert!(error.message.contains("unterminated string"));
    }
}
