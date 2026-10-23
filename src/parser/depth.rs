//! Iterative validation also covers left-associated/postfix chains, which do
//! not necessarily nest the parser. Run before cloning/evaluating host ASTs.

use super::ast::{Expr, Stmt};
use crate::lexer::Span;

#[derive(Clone, Copy)]
pub(crate) enum Node<'a> {
    Statement(&'a Stmt),
    Expression(&'a Expr),
}

pub(crate) fn check_depth(root: Node<'_>, maximum: usize) -> Result<(), Option<Span>> {
    use Node::{Expression as E, Statement as S};
    let mut pending = vec![(root, 1, None)];
    while let Some((node, depth, mut span)) = pending.pop() {
        if let S(Stmt::Located { span: located, .. }) = node {
            span = Some(*located);
        }
        if depth > maximum {
            return Err(span);
        }
        let mut push = |node| pending.push((node, depth + 1, span));
        match node {
            S(Stmt::Located { statement, .. }) => push(S(statement)),
            S(Stmt::Function { function, .. }) | E(Expr::Function(function)) => {
                for statement in &function.body {
                    push(S(statement));
                }
            }
            S(Stmt::Declaration { initializer, .. }) => {
                if let Some(expression) = initializer {
                    push(E(expression));
                }
            }
            S(Stmt::Assignment { target, value }) => {
                push(E(target));
                push(E(value));
            }
            S(Stmt::Expression(expression)) => push(E(expression)),
            S(Stmt::Return(values)) | E(Expr::List(values)) => {
                for expression in values {
                    push(E(expression));
                }
            }
            S(Stmt::TryCatch {
                body, catch_body, ..
            }) => {
                for statement in body.iter().chain(catch_body) {
                    push(S(statement));
                }
            }
            S(Stmt::While { condition, body })
            | S(Stmt::CollectionFor {
                iterable: condition,
                body,
                ..
            }) => {
                push(E(condition));
                for statement in body {
                    push(S(statement));
                }
            }
            S(Stmt::NumericFor {
                start,
                limit,
                step,
                body,
                ..
            }) => {
                push(E(start));
                push(E(limit));
                if let Some(step) = step {
                    push(E(step));
                }
                for statement in body {
                    push(S(statement));
                }
            }
            S(Stmt::If {
                branches,
                else_branch,
            }) => {
                for (condition, body) in branches {
                    push(E(condition));
                    for statement in body {
                        push(S(statement));
                    }
                }
                if let Some(body) = else_branch {
                    for statement in body {
                        push(S(statement));
                    }
                }
            }
            E(Expr::Unary { operand, .. }) => push(E(operand)),
            E(Expr::Binary { left, right, .. }) => {
                push(E(left));
                push(E(right));
            }
            E(Expr::Call { callee, arguments }) => {
                push(E(callee));
                for argument in arguments {
                    push(E(argument));
                }
            }
            E(Expr::Index { object, index }) => {
                push(E(object));
                push(E(index));
            }
            E(Expr::Property { object, .. }) => push(E(object)),
            E(Expr::Object(properties)) => {
                for (_, expression) in properties {
                    push(E(expression));
                }
            }
            S(Stmt::Import { .. } | Stmt::Break | Stmt::Continue)
            | E(
                Expr::Nil
                | Expr::Boolean(_)
                | Expr::Number(_)
                | Expr::String(_)
                | Expr::Identifier(_),
            ) => {}
        }
    }
    Ok(())
}
