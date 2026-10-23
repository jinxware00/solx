//! Solx parser.
//!
//! Converts lexer tokens into an abstract syntax tree.

pub mod ast;

use std::fmt;
use std::mem::discriminant;

use crate::lexer::{LexError, Span, Token, TokenKind, tokenize};
use ast::{BinaryOp, DeclarationKind, Expr, Function, Program, Stmt, UnaryOp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}:{}",
            self.message, self.span.line, self.span.column
        )
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(error: LexError) -> Self {
        Self {
            message: error.message,
            span: Span {
                start: 0,
                end: 0,
                line: error.line,
                column: error.column,
            },
        }
    }
}

/// Tokenizes and parses a complete Solx source file.
pub fn parse(source: &str) -> Result<Program, ParseError> {
    let tokens = tokenize(source)?;
    Parser::new(tokens).parse_program()
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
    function_depth: usize,
    loop_depth: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            current: 0,
            function_depth: 0,
            loop_depth: 0,
        }
    }

    fn parse_program(mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            if self.matches(&TokenKind::Semicolon) {
                continue;
            }
            statements.push(self.parse_statement()?);
        }

        Ok(Program { statements })
    }

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        if self.check(&TokenKind::Fn)
            && !matches!(self.tokens[self.current + 1].kind, TokenKind::LeftParen)
        {
            self.advance();
            let name = self.consume_identifier("expected a function name after 'fn'")?;
            let function = self.parse_function()?;
            return Ok(Stmt::Function { name, function });
        }

        if self.matches(&TokenKind::Var) {
            return self.parse_declaration(DeclarationKind::Var);
        }

        if self.matches(&TokenKind::Global) {
            return self.parse_declaration(DeclarationKind::Global);
        }

        if self.matches(&TokenKind::Const) {
            return self.parse_declaration(DeclarationKind::Const);
        }

        if self.matches(&TokenKind::If) {
            return self.parse_if();
        }

        if self.matches(&TokenKind::While) {
            return self.parse_while();
        }

        if self.matches(&TokenKind::For) {
            return self.parse_for();
        }

        if self.matches(&TokenKind::Break) {
            if self.loop_depth == 0 {
                return Err(self.error_at_previous("break is only allowed inside a loop"));
            }
            return Ok(Stmt::Break);
        }

        if self.matches(&TokenKind::Continue) {
            if self.loop_depth == 0 {
                return Err(self.error_at_previous("continue is only allowed inside a loop"));
            }
            return Ok(Stmt::Continue);
        }

        if self.matches(&TokenKind::Return) {
            if self.function_depth == 0 {
                return Err(self.error_at_previous("return is only allowed inside a function"));
            }
            return self.parse_return();
        }

        let expression = self.parse_expression(0)?;

        if self.matches(&TokenKind::Assign) {
            let value = self.parse_expression(0)?;

            if !matches!(
                expression,
                Expr::Identifier(_) | Expr::Property { .. } | Expr::Index { .. }
            ) {
                return Err(self.error_at_previous("invalid assignment target"));
            }

            return Ok(Stmt::Assignment {
                target: expression,
                value,
            });
        }

        Ok(Stmt::Expression(expression))
    }

    fn parse_declaration(&mut self, kind: DeclarationKind) -> Result<Stmt, ParseError> {
        let name = self.consume_identifier("expected a variable name after declaration")?;

        let type_annotation = if self.matches(&TokenKind::Colon) {
            Some(self.consume_identifier("expected a type name after ':'")?)
        } else {
            None
        };

        let initializer = if self.matches(&TokenKind::Assign) {
            Some(self.parse_expression(0)?)
        } else {
            None
        };

        Ok(Stmt::Declaration {
            kind,
            name,
            type_annotation,
            initializer,
        })
    }

    fn parse_function(&mut self) -> Result<Function, ParseError> {
        self.consume(
            &TokenKind::LeftParen,
            "expected '(' before function parameters",
        )?;
        let mut parameters = Vec::new();

        if !self.check(&TokenKind::RightParen) {
            loop {
                let name = self.consume_identifier("expected a parameter name")?;
                if parameters.contains(&name) {
                    return Err(self.error_at_previous(format!("duplicate parameter '{name}'")));
                }
                parameters.push(name);

                if !self.matches(&TokenKind::Comma) {
                    break;
                }
            }
        }

        self.consume(
            &TokenKind::RightParen,
            "expected ')' after function parameters",
        )?;
        self.function_depth += 1;
        // Loop control cannot cross a function boundary.
        let enclosing_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let body = self.parse_block();
        self.loop_depth = enclosing_loop_depth;
        self.function_depth -= 1;
        let body = body?;
        self.consume(&TokenKind::End, "expected 'end' to close function")?;

        Ok(Function { parameters, body })
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        let mut values = Vec::new();

        // A newline immediately after the keyword makes this a bare return.
        if self.peek().span.line > self.tokens[self.current - 1].span.line
            || self.is_at_end()
            || self.check(&TokenKind::End)
            || self.check(&TokenKind::Else)
            || self.check(&TokenKind::ElseIf)
            || self.check(&TokenKind::Semicolon)
        {
            return Ok(Stmt::Return(values));
        }

        values.push(self.parse_expression(0)?);

        if self.check(&TokenKind::Comma) {
            return Err(self.error("multiple return values are not supported yet"));
        }

        Ok(Stmt::Return(values))
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        let mut branches = Vec::new();

        let condition = self.parse_expression(0)?;
        self.consume(&TokenKind::Then, "expected 'then' after if condition")?;

        let body = self.parse_block()?;
        branches.push((condition, body));

        while self.matches(&TokenKind::ElseIf) {
            let condition = self.parse_expression(0)?;

            self.consume(&TokenKind::Then, "expected 'then' after elseif condition")?;

            let body = self.parse_block()?;
            branches.push((condition, body));
        }

        let else_branch = if self.matches(&TokenKind::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };

        self.consume(&TokenKind::End, "expected 'end' to close if statement")?;

        Ok(Stmt::If {
            branches,
            else_branch,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        let condition = self.parse_expression(0)?;
        self.consume(&TokenKind::Do, "expected 'do' after while condition")?;
        let body = self.parse_loop_body("expected 'end' to close while loop")?;
        Ok(Stmt::While { condition, body })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        let name = self.consume_identifier("expected a loop variable after 'for'")?;
        self.consume(&TokenKind::Assign, "expected '=' after for loop variable")?;
        let start = self.parse_expression(0)?;
        self.consume(&TokenKind::Comma, "expected ',' after for loop start")?;
        let limit = self.parse_expression(0)?;
        let step = if self.matches(&TokenKind::Comma) {
            Some(self.parse_expression(0)?)
        } else {
            None
        };
        self.consume(&TokenKind::Do, "expected 'do' after for loop bounds")?;
        let body = self.parse_loop_body("expected 'end' to close for loop")?;
        Ok(Stmt::NumericFor {
            name,
            start,
            limit,
            step,
            body,
        })
    }

    fn parse_loop_body(&mut self, end_message: &str) -> Result<Vec<Stmt>, ParseError> {
        self.loop_depth += 1;
        let body = self.parse_block();
        self.loop_depth -= 1;
        let body = body?;
        self.consume(&TokenKind::End, end_message)?;
        Ok(body)
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut statements = Vec::new();

        while !self.is_at_end()
            && !self.check(&TokenKind::Else)
            && !self.check(&TokenKind::ElseIf)
            && !self.check(&TokenKind::End)
        {
            if self.matches(&TokenKind::Semicolon) {
                continue;
            }
            statements.push(self.parse_statement()?);
        }

        Ok(statements)
    }

    // Pratt parser: binding powers encode operator precedence
    // and associativity.
    fn parse_expression(&mut self, min_binding_power: u8) -> Result<Expr, ParseError> {
        let mut left = self.parse_prefix()?;

        loop {
            // Function calls and property/index access bind tightly.
            if 14 < min_binding_power {
                break;
            }

            if self.matches(&TokenKind::LeftParen) {
                let mut arguments = Vec::new();

                if !self.check(&TokenKind::RightParen) {
                    arguments.push(self.parse_expression(0)?);

                    while self.matches(&TokenKind::Comma) {
                        arguments.push(self.parse_expression(0)?);
                    }
                }

                self.consume(
                    &TokenKind::RightParen,
                    "expected ')' after function arguments",
                )?;

                left = Expr::Call {
                    callee: Box::new(left),
                    arguments,
                };

                continue;
            }

            if self.matches(&TokenKind::LeftBracket) {
                let index = self.parse_expression(0)?;

                self.consume(&TokenKind::RightBracket, "expected ']' after index")?;

                left = Expr::Index {
                    object: Box::new(left),
                    index: Box::new(index),
                };

                continue;
            }

            if self.matches(&TokenKind::Dot) {
                let name = self.consume_identifier("expected property name after '.'")?;

                left = Expr::Property {
                    object: Box::new(left),
                    name,
                };

                continue;
            }

            let Some((operator, left_bp, right_bp)) = self.infix_binding_power() else {
                break;
            };

            if left_bp < min_binding_power {
                break;
            }

            self.advance();

            let right = self.parse_expression(right_bp)?;

            left = Expr::Binary {
                left: Box::new(left),
                op: operator,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        if self.matches(&TokenKind::Fn) {
            return self.parse_function().map(Expr::Function);
        }

        if self.matches(&TokenKind::Minus) {
            let operand = self.parse_expression(11)?;

            return Ok(Expr::Unary {
                op: UnaryOp::Negate,
                operand: Box::new(operand),
            });
        }

        if self.matches(&TokenKind::Not) {
            let operand = self.parse_expression(11)?;

            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                operand: Box::new(operand),
            });
        }

        if self.matches(&TokenKind::Hash) {
            let operand = self.parse_expression(11)?;

            return Ok(Expr::Unary {
                op: UnaryOp::Length,
                operand: Box::new(operand),
            });
        }

        if self.matches(&TokenKind::LeftParen) {
            let expression = self.parse_expression(0)?;

            self.consume(&TokenKind::RightParen, "expected ')' after expression")?;

            return Ok(expression);
        }

        if self.matches(&TokenKind::LeftBracket) {
            return self.parse_list();
        }

        if self.matches(&TokenKind::LeftBrace) {
            return self.parse_object();
        }

        let token = self.advance().clone();

        match token.kind {
            TokenKind::Nil => Ok(Expr::Nil),
            TokenKind::True => Ok(Expr::Boolean(true)),
            TokenKind::False => Ok(Expr::Boolean(false)),
            TokenKind::Number(value) => Ok(Expr::Number(value)),
            TokenKind::String(value) => Ok(Expr::String(value)),
            TokenKind::Identifier(name) => Ok(Expr::Identifier(name)),
            _ => Err(ParseError {
                message: format!("expected expression, found {:?}", token.kind),
                span: token.span,
            }),
        }
    }

    fn parse_list(&mut self) -> Result<Expr, ParseError> {
        let mut items = Vec::new();

        if !self.check(&TokenKind::RightBracket) {
            items.push(self.parse_expression(0)?);

            while self.matches(&TokenKind::Comma) {
                if self.check(&TokenKind::RightBracket) {
                    break;
                }

                items.push(self.parse_expression(0)?);
            }
        }

        self.consume(&TokenKind::RightBracket, "expected ']' after list items")?;

        Ok(Expr::List(items))
    }

    fn parse_object(&mut self) -> Result<Expr, ParseError> {
        let mut properties = Vec::new();

        if !self.check(&TokenKind::RightBrace) {
            loop {
                let key = if let TokenKind::Identifier(name) = &self.peek().kind {
                    let name = name.clone();
                    self.advance();
                    name
                } else if let TokenKind::String(value) = &self.peek().kind {
                    let value = value.clone();
                    self.advance();
                    value
                } else {
                    return Err(self.error("expected an object property name"));
                };

                self.consume(
                    &TokenKind::Assign,
                    "expected '=' after object property name",
                )?;

                let value = self.parse_expression(0)?;
                properties.push((key, value));

                if !self.matches(&TokenKind::Comma) {
                    break;
                }

                if self.check(&TokenKind::RightBrace) {
                    break;
                }
            }
        }

        self.consume(
            &TokenKind::RightBrace,
            "expected '}' after object properties",
        )?;

        Ok(Expr::Object(properties))
    }

    fn infix_binding_power(&self) -> Option<(BinaryOp, u8, u8)> {
        let result = match self.peek().kind {
            TokenKind::Or => (BinaryOp::Or, 1, 2),
            TokenKind::And => (BinaryOp::And, 3, 4),

            TokenKind::Equal => (BinaryOp::Equal, 5, 6),
            TokenKind::NotEqual => (BinaryOp::NotEqual, 5, 6),
            TokenKind::Less => (BinaryOp::Less, 5, 6),
            TokenKind::LessEqual => (BinaryOp::LessEqual, 5, 6),
            TokenKind::Greater => (BinaryOp::Greater, 5, 6),
            TokenKind::GreaterEqual => (BinaryOp::GreaterEqual, 5, 6),

            // Concatenation is right-associative in Lua.
            TokenKind::Concat => (BinaryOp::Concatenate, 7, 7),

            TokenKind::Plus => (BinaryOp::Add, 8, 9),
            TokenKind::Minus => (BinaryOp::Subtract, 8, 9),

            TokenKind::Star => (BinaryOp::Multiply, 10, 11),
            TokenKind::Slash => (BinaryOp::Divide, 10, 11),
            TokenKind::FloorSlash => (BinaryOp::FloorDivide, 10, 11),
            TokenKind::Percent => (BinaryOp::Modulo, 10, 11),

            // Exponentiation is right-associative and binds tighter
            // than unary minus, matching Lua's precedence.
            TokenKind::Caret => (BinaryOp::Power, 12, 12),

            _ => return None,
        };

        Some(result)
    }

    fn consume_identifier(&mut self, message: &str) -> Result<String, ParseError> {
        if let TokenKind::Identifier(name) = &self.peek().kind {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            Err(self.error(message))
        }
    }

    fn consume(&mut self, expected: &TokenKind, message: &str) -> Result<(), ParseError> {
        if self.check(expected) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn matches(&mut self, expected: &TokenKind) -> bool {
        if self.check(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, expected: &TokenKind) -> bool {
        discriminant(&self.peek().kind) == discriminant(expected)
    }

    fn advance(&mut self) -> &Token {
        let index = self.current;
        if !self.is_at_end() {
            self.current += 1;
        }

        &self.tokens[index]
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn is_at_end(&self) -> bool {
        self.check(&TokenKind::Eof)
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            span: self.peek().span,
        }
    }

    fn error_at_previous(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            span: self.tokens[self.current - 1].span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_variable_declarations_and_assignment() {
        let program = parse("var score = 50\nscore = score + 10").unwrap();

        assert_eq!(program.statements.len(), 2);

        assert!(matches!(
            &program.statements[0],
            Stmt::Declaration {
                kind: DeclarationKind::Var,
                name,
                ..
            } if name == "score"
        ));

        assert!(matches!(&program.statements[1], Stmt::Assignment { .. }));
    }

    #[test]
    fn multiplication_has_higher_precedence_than_addition() {
        let program = parse("var result = 2 + 3 * 4").unwrap();

        let Stmt::Declaration {
            initializer: Some(expression),
            ..
        } = &program.statements[0]
        else {
            panic!("expected declaration with initializer");
        };

        let Expr::Binary {
            op: BinaryOp::Add,
            right,
            ..
        } = expression
        else {
            panic!("expected addition at the root");
        };

        assert!(matches!(
            right.as_ref(),
            Expr::Binary {
                op: BinaryOp::Multiply,
                ..
            }
        ));
    }

    #[test]
    fn exponentiation_is_right_associative() {
        let program = parse("var result = 2 ^ 3 ^ 2").unwrap();

        let Stmt::Declaration {
            initializer: Some(expression),
            ..
        } = &program.statements[0]
        else {
            panic!("expected declaration");
        };

        assert!(matches!(
            expression,
            Expr::Binary {
                op: BinaryOp::Power,
                right,
                ..
            } if matches!(
                right.as_ref(),
                Expr::Binary {
                    op: BinaryOp::Power,
                    ..
                }
            )
        ));
    }

    #[test]
    fn parses_if_elseif_else() {
        let source = "
            if score >= 90 then
                print(\"A\")
            elseif score >= 70 then
                print(\"B\")
            else
                print(\"C\")
            end
        ";

        let program = parse(source).unwrap();

        assert!(matches!(
            &program.statements[0],
            Stmt::If {
                branches,
                else_branch: Some(_),
            } if branches.len() == 2
        ));
    }

    #[test]
    fn parses_lists_objects_and_property_access() {
        let source = r#"
            var items = ["Sword", "Shield"]
            var player = { name = "Jinx", score = 100 }
            print(player.name)
        "#;

        let program = parse(source).unwrap();
        assert_eq!(program.statements.len(), 3);

        assert!(matches!(
            &program.statements[0],
            Stmt::Declaration {
                initializer: Some(Expr::List(items)),
                ..
            } if items.len() == 2
        ));

        assert!(matches!(
            &program.statements[1],
            Stmt::Declaration {
                initializer: Some(Expr::Object(properties)),
                ..
            } if properties.len() == 2
        ));
    }

    #[test]
    fn rejects_missing_then() {
        let error = parse("if true print(\"oops\") end").unwrap_err();
        assert!(error.message.contains("expected 'then'"));
    }

    #[test]
    fn parses_type_annotations() {
        let program = parse("var score: Number = 100").unwrap();

        assert!(matches!(
            &program.statements[0],
            Stmt::Declaration {
                type_annotation: Some(type_name),
                ..
            } if type_name == "Number"
        ));
    }
}
