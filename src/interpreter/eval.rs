use std::collections::HashMap;
use std::ops::ControlFlow;
use std::rc::Rc;

use crate::parser::ast::{BinaryOp, DeclarationKind, Expr, Function, Program, Stmt, UnaryOp};
use crate::parser::parse;
use crate::runtime::value::{NativeFunction, UserFunction};
use crate::runtime::{Environment, RuntimeError, Value};

enum Jump {
    Return(Value),
    Break,
    Continue,
}

type ExecutionResult = Result<ControlFlow<Jump>, RuntimeError>;

pub struct Interpreter {
    environment: Environment,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    pub fn new() -> Self {
        let mut environment = Environment::new();
        for function in [
            NativeFunction::Print,
            NativeFunction::Str,
            NativeFunction::Num,
        ] {
            environment
                .define(
                    function.name().to_owned(),
                    Value::NativeFunction(function),
                    false,
                    None,
                    true,
                )
                .expect("built-in function definitions are valid");
        }
        Self { environment }
    }

    pub fn run_source(&mut self, source: &str) -> Result<(), RuntimeError> {
        let program = parse(source).map_err(|error| RuntimeError::new(error.to_string()))?;

        self.run_program(&program)
    }

    pub fn run_program(&mut self, program: &Program) -> Result<(), RuntimeError> {
        match self.execute_statements(&program.statements)? {
            ControlFlow::Continue(()) => Ok(()),
            ControlFlow::Break(jump) => Err(Self::unexpected_jump(jump)),
        }
    }

    pub fn get_global(&self, name: &str) -> Option<Value> {
        self.environment.get_global(name)
    }

    fn execute(&mut self, statement: &Stmt) -> ExecutionResult {
        match statement {
            Stmt::Function { name, function } => {
                let value = self.make_function(Some(name.clone()), function);
                self.environment
                    .define(name.clone(), value, false, None, false)?;
            }

            Stmt::Declaration {
                kind,
                name,
                type_annotation,
                initializer,
            } => {
                let value = match initializer {
                    Some(expression) => self.evaluate(expression)?,
                    None => Value::Nil,
                };

                self.environment.define(
                    name.clone(),
                    value,
                    matches!(kind, DeclarationKind::Const),
                    type_annotation.clone(),
                    matches!(kind, DeclarationKind::Global),
                )?;
            }

            Stmt::Assignment { target, value } => {
                let value = self.evaluate(value)?;
                self.assign_target(target, value)?;
            }

            Stmt::Expression(expression) => {
                self.evaluate(expression)?;
            }

            Stmt::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    if self.evaluate(condition)?.is_truthy() {
                        return self.execute_block(body);
                    }
                }

                if let Some(body) = else_branch {
                    return self.execute_block(body);
                }
            }

            Stmt::While { condition, body } => {
                while self.evaluate(condition)?.is_truthy() {
                    match self.execute_block(body)? {
                        ControlFlow::Break(Jump::Break) => break,
                        flow @ ControlFlow::Break(Jump::Return(_)) => return Ok(flow),
                        ControlFlow::Break(Jump::Continue) | ControlFlow::Continue(()) => {}
                    }
                }
            }

            Stmt::NumericFor {
                name,
                start,
                limit,
                step,
                body,
            } => {
                return self.execute_for(name, start, limit, step.as_ref(), body);
            }

            Stmt::Break => return Ok(ControlFlow::Break(Jump::Break)),
            Stmt::Continue => return Ok(ControlFlow::Break(Jump::Continue)),

            Stmt::Return(values) => {
                let value = match values.as_slice() {
                    [] => Value::Nil,
                    [expression] => self.evaluate(expression)?,
                    _ => {
                        return Err(RuntimeError::new(
                            "multiple return values are not supported yet",
                        ));
                    }
                };
                return Ok(ControlFlow::Break(Jump::Return(value)));
            }
        }

        Ok(ControlFlow::Continue(()))
    }

    fn execute_statements(&mut self, statements: &[Stmt]) -> ExecutionResult {
        for statement in statements {
            if let ControlFlow::Break(value) = self.execute(statement)? {
                return Ok(ControlFlow::Break(value));
            }
        }
        Ok(ControlFlow::Continue(()))
    }

    fn execute_block(&mut self, statements: &[Stmt]) -> ExecutionResult {
        self.environment.begin_scope();
        let result = self.execute_statements(statements);
        self.environment.end_scope();
        result
    }

    fn execute_for(
        &mut self,
        name: &str,
        start: &Expr,
        limit: &Expr,
        step: Option<&Expr>,
        body: &[Stmt],
    ) -> ExecutionResult {
        // Evaluate the header once in the enclosing scope, before binding the counter.
        let start = self.evaluate(start)?;
        let limit = self.evaluate(limit)?;
        let step = match step {
            Some(expression) => self.evaluate(expression)?,
            None => Value::Number(1.0),
        };
        let mut counter = self.require_finite_number(&start, "for loop start")?;
        let limit = self.require_finite_number(&limit, "for loop limit")?;
        let step = self.require_finite_number(&step, "for loop step")?;
        if step == 0.0 {
            return Err(RuntimeError::new("for loop step must not be zero"));
        }

        while if step > 0.0 {
            counter <= limit
        } else {
            counter >= limit
        } {
            // A new binding per iteration keeps escaped closures independent.
            self.environment.begin_scope();
            let result = self
                .environment
                .define(name.to_owned(), Value::Number(counter), false, None, false)
                .and_then(|()| self.execute_statements(body));
            self.environment.end_scope();
            match result? {
                ControlFlow::Break(Jump::Break) => break,
                flow @ ControlFlow::Break(Jump::Return(_)) => return Ok(flow),
                ControlFlow::Break(Jump::Continue) | ControlFlow::Continue(()) => {}
            }

            if counter == limit {
                break;
            }
            let next = counter + step;
            if next == counter {
                return Err(RuntimeError::new(
                    "for loop step does not advance the counter",
                ));
            }
            counter = next;
        }
        Ok(ControlFlow::Continue(()))
    }

    fn require_finite_number(&self, value: &Value, operation: &str) -> Result<f64, RuntimeError> {
        let number = self.require_number(value, operation)?;
        if number.is_finite() {
            Ok(number)
        } else {
            Err(RuntimeError::new(format!("{operation} must be finite")))
        }
    }

    fn unexpected_jump(jump: Jump) -> RuntimeError {
        RuntimeError::new(match jump {
            Jump::Return(_) => "return is only allowed inside a function",
            Jump::Break => "break is only allowed inside a loop",
            Jump::Continue => "continue is only allowed inside a loop",
        })
    }

    fn make_function(&self, name: Option<String>, definition: &Function) -> Value {
        Value::Function(Rc::new(UserFunction {
            name,
            definition: definition.clone(),
            closure: self.environment.clone(),
        }))
    }

    fn evaluate(&mut self, expression: &Expr) -> Result<Value, RuntimeError> {
        match expression {
            Expr::Function(function) => Ok(self.make_function(None, function)),
            Expr::Nil => Ok(Value::Nil),
            Expr::Boolean(value) => Ok(Value::Boolean(*value)),
            Expr::Number(value) => {
                let number = value
                    .parse::<f64>()
                    .map_err(|_| RuntimeError::new(format!("invalid number '{value}'")))?;

                Ok(Value::Number(number))
            }
            Expr::String(value) => Ok(Value::String(value.clone())),
            Expr::Identifier(name) => self.environment.get(name),

            Expr::Unary { op, operand } => {
                let value = self.evaluate(operand)?;

                match op {
                    UnaryOp::Negate => {
                        let number = self.require_number(&value, "unary '-'")?;
                        Ok(Value::Number(-number))
                    }
                    UnaryOp::Not => Ok(Value::Boolean(!value.is_truthy())),
                    UnaryOp::Length => match value {
                        Value::String(text) => Ok(Value::Number(text.chars().count() as f64)),
                        Value::List(items) => Ok(Value::Number(items.borrow().len() as f64)),
                        Value::Object(properties) => {
                            Ok(Value::Number(properties.borrow().len() as f64))
                        }
                        other => Err(RuntimeError::new(format!(
                            "operator '#' does not support {}",
                            other.type_name()
                        ))),
                    },
                }
            }

            Expr::Binary { left, op, right } => {
                let left = self.evaluate(left)?;

                // Lua-style short-circuit operators return operands.
                if matches!(op, BinaryOp::And) && !left.is_truthy() {
                    return Ok(left);
                }

                if matches!(op, BinaryOp::Or) && left.is_truthy() {
                    return Ok(left);
                }

                let right = self.evaluate(right)?;
                self.evaluate_binary(left, op, right)
            }

            Expr::Call { callee, arguments } => self.evaluate_call(callee, arguments),

            Expr::Index { object, index } => {
                let object = self.evaluate(object)?;
                let index = self.evaluate(index)?;
                self.read_index(object, index)
            }

            Expr::Property { object, name } => {
                let object = self.evaluate(object)?;
                self.read_property(object, name)
            }

            Expr::List(items) => {
                let values = items
                    .iter()
                    .map(|item| self.evaluate(item))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(Value::list(values))
            }

            Expr::Object(properties) => {
                let mut values = HashMap::new();

                for (key, expression) in properties {
                    values.insert(key.clone(), self.evaluate(expression)?);
                }

                Ok(Value::object(values))
            }
        }
    }

    fn evaluate_call(&mut self, callee: &Expr, arguments: &[Expr]) -> Result<Value, RuntimeError> {
        let callee = self.evaluate(callee)?;
        let values = arguments
            .iter()
            .map(|argument| self.evaluate(argument))
            .collect::<Result<Vec<_>, _>>()?;

        match callee {
            Value::Function(function) => {
                self.require_argument_count(
                    function.name.as_deref().unwrap_or("<anonymous>"),
                    &values,
                    function.definition.parameters.len(),
                )?;

                // Only the definition's scopes and a fresh call scope are visible.
                let mut environment = function.closure.clone();
                environment.begin_scope();
                for (name, value) in function.definition.parameters.iter().zip(values) {
                    environment.define(name.clone(), value, false, None, false)?;
                }

                let caller = std::mem::replace(&mut self.environment, environment);
                let result = self.execute_statements(&function.definition.body);
                // Restore the caller on both returns and runtime errors.
                self.environment = caller;
                match result? {
                    ControlFlow::Break(Jump::Return(value)) => Ok(value),
                    ControlFlow::Continue(()) => Ok(Value::Nil),
                    ControlFlow::Break(jump) => Err(Self::unexpected_jump(jump)),
                }
            }
            Value::NativeFunction(function) => self.call_native(function, &values),
            other => Err(RuntimeError::new(format!(
                "cannot call {}; expected Function",
                other.type_name()
            ))),
        }
    }

    fn call_native(
        &self,
        function: NativeFunction,
        values: &[Value],
    ) -> Result<Value, RuntimeError> {
        let name = function.name();
        match function {
            NativeFunction::Print => {
                println!(
                    "{}",
                    values
                        .iter()
                        .map(Value::to_solx_string)
                        .collect::<Vec<_>>()
                        .join("\t")
                );

                Ok(Value::Nil)
            }

            NativeFunction::Str => {
                self.require_argument_count(name, values, 1)?;
                Ok(Value::String(values[0].to_solx_string()))
            }

            NativeFunction::Num => {
                self.require_argument_count(name, values, 1)?;

                match &values[0] {
                    Value::Number(number) => Ok(Value::Number(*number)),
                    Value::String(text) => {
                        let number = text.trim().parse::<f64>().map_err(|_| {
                            RuntimeError::new(format!("cannot convert '{text}' to Number"))
                        })?;

                        Ok(Value::Number(number))
                    }
                    other => Err(RuntimeError::new(format!(
                        "num() cannot convert {} to Number",
                        other.type_name()
                    ))),
                }
            }
        }
    }

    fn require_argument_count(
        &self,
        name: &str,
        arguments: &[Value],
        expected: usize,
    ) -> Result<(), RuntimeError> {
        if arguments.len() == expected {
            Ok(())
        } else {
            Err(RuntimeError::new(format!(
                "{name}() expects {expected} argument(s), got {}",
                arguments.len()
            )))
        }
    }

    fn evaluate_binary(
        &self,
        left: Value,
        op: &BinaryOp,
        right: Value,
    ) -> Result<Value, RuntimeError> {
        use BinaryOp::*;

        match op {
            Add | Subtract | Multiply | Divide | FloorDivide | Modulo | Power => {
                let a = self.require_number(&left, "arithmetic")?;
                let b = self.require_number(&right, "arithmetic")?;

                let result = match op {
                    Add => a + b,
                    Subtract => a - b,
                    Multiply => a * b,
                    Divide => a / b,
                    FloorDivide => (a / b).floor(),
                    Modulo => a - (a / b).floor() * b,
                    Power => a.powf(b),
                    _ => unreachable!(),
                };

                Ok(Value::Number(result))
            }

            Concatenate => Ok(Value::String(format!(
                "{}{}",
                left.to_solx_string(),
                right.to_solx_string()
            ))),

            Equal => Ok(Value::Boolean(left == right)),
            NotEqual => Ok(Value::Boolean(left != right)),

            Less | LessEqual | Greater | GreaterEqual => {
                let ordering = match (&left, &right) {
                    (Value::Number(a), Value::Number(b)) => a.partial_cmp(b),
                    (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
                    _ => {
                        return Err(RuntimeError::new(format!(
                            "cannot compare {} with {}",
                            left.type_name(),
                            right.type_name()
                        )));
                    }
                };

                let result = match (op, ordering) {
                    (_, None) => false,
                    (Less, Some(ordering)) => ordering.is_lt(),
                    (LessEqual, Some(ordering)) => !ordering.is_gt(),
                    (Greater, Some(ordering)) => ordering.is_gt(),
                    (GreaterEqual, Some(ordering)) => !ordering.is_lt(),
                    _ => unreachable!(),
                };

                Ok(Value::Boolean(result))
            }

            And | Or => Ok(right),
        }
    }

    fn require_number(&self, value: &Value, operation: &str) -> Result<f64, RuntimeError> {
        value.as_number().ok_or_else(|| {
            RuntimeError::new(format!(
                "{operation} requires Number, got {}",
                value.type_name()
            ))
        })
    }

    fn read_index(&self, object: Value, index: Value) -> Result<Value, RuntimeError> {
        match object {
            Value::List(items) => {
                let index = match index {
                    Value::Number(number)
                        if number.is_finite()
                            && number.fract() == 0.0
                            && number >= 1.0
                            && number <= usize::MAX as f64 =>
                    {
                        number as usize
                    }
                    _ => {
                        eprintln!("Warning: invalid list index");
                        return Ok(Value::Nil);
                    }
                };

                let items = items.borrow();

                match items.get(index - 1) {
                    Some(value) => Ok(value.clone()),
                    None => {
                        eprintln!("Warning: list index {index} is out of range");
                        Ok(Value::Nil)
                    }
                }
            }

            Value::Object(properties) => {
                let key = match index {
                    Value::String(key) => key,
                    _ => {
                        return Err(RuntimeError::new("object index must be a String"));
                    }
                };

                match properties.borrow().get(&key) {
                    Some(value) => Ok(value.clone()),
                    None => {
                        eprintln!("Warning: missing object property '{key}'");
                        Ok(Value::Nil)
                    }
                }
            }

            other => Err(RuntimeError::new(format!(
                "cannot index {}",
                other.type_name()
            ))),
        }
    }

    fn read_property(&self, object: Value, name: &str) -> Result<Value, RuntimeError> {
        match object {
            Value::Object(properties) => match properties.borrow().get(name) {
                Some(value) => Ok(value.clone()),
                None => {
                    eprintln!("Warning: missing object property '{name}'");
                    Ok(Value::Nil)
                }
            },

            other => Err(RuntimeError::new(format!(
                "cannot access property '{name}' on {}",
                other.type_name()
            ))),
        }
    }

    fn assign_target(&mut self, target: &Expr, value: Value) -> Result<(), RuntimeError> {
        match target {
            Expr::Identifier(name) => self.environment.assign(name, value),

            Expr::Property { object, name } => {
                let object = self.evaluate(object)?;

                let Value::Object(properties) = object else {
                    return Err(RuntimeError::new("property assignment requires an Object"));
                };

                properties.borrow_mut().insert(name.clone(), value);
                Ok(())
            }

            Expr::Index { object, index } => {
                let object = self.evaluate(object)?;
                let index = self.evaluate(index)?;

                match object {
                    Value::List(items) => {
                        let Value::Number(number) = index else {
                            return Err(RuntimeError::new("list index must be a number"));
                        };

                        if !number.is_finite() || number.fract() != 0.0 || number < 1.0 {
                            return Err(RuntimeError::new("list index must be a positive integer"));
                        }

                        let position = number as usize - 1;
                        let mut items = items.borrow_mut();

                        if position >= items.len() {
                            return Err(RuntimeError::new(
                                "list assignment index is out of bounds",
                            ));
                        }

                        items[position] = value;
                        Ok(())
                    }

                    Value::Object(properties) => {
                        let Value::String(key) = index else {
                            return Err(RuntimeError::new("object index must be a string"));
                        };

                        properties.borrow_mut().insert(key, value);
                        Ok(())
                    }

                    _ => Err(RuntimeError::new(
                        "index assignment requires a List or Object",
                    )),
                }
            }

            _ => Err(RuntimeError::new("invalid assignment target")),
        }
    }
}
