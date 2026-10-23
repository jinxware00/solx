use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::Path;
use std::rc::Rc;

use super::control::Control;
use crate::limits::{CancellationToken, ExecutionLimits, LimitKind};
use crate::modules::{ModuleId, ModuleLoader};
use crate::parser::ast::{BinaryOp, DeclarationKind, Expr, Function, Program, Stmt, UnaryOp};
use crate::parser::{
    depth::{Node, check_depth},
    parse_with_limits,
};
use crate::runtime::error::{RuntimeErrorKind, SourceLocation};
use crate::runtime::value::{ModuleValue, NativeFunction, UserFunction};
use crate::runtime::{Environment, RuntimeError, Value, read_collection, write_collection};

enum Jump {
    Return(Value),
    Break,
    Continue,
}

type ExecutionResult = Result<ControlFlow<Jump>, RuntimeError>;

pub struct Interpreter {
    environment: Environment,
    modules: ModuleLoader,
    current_location: Option<SourceLocation>,
    control: Control,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Interpreter {
    fn drop(&mut self) {
        // Drop only this interpreter's roots. Host-held Values and captured
        // environments remain roots and must retain their complete contents.
        self.environment.release_roots();
        self.modules.release_roots();
        crate::runtime::collect_garbage();
    }
}

impl Interpreter {
    pub fn new() -> Self {
        Self::with_limits(ExecutionLimits::default()).expect("default limits are valid")
    }

    pub fn with_limits(limits: ExecutionLimits) -> Result<Self, RuntimeError> {
        limits.validate().map_err(RuntimeError::new)?;
        Ok(Self {
            environment: Self::fresh_environment(),
            modules: ModuleLoader::default(),
            current_location: None,
            control: Control::new(limits),
        })
    }

    pub fn limits(&self) -> ExecutionLimits {
        self.control.limits
    }

    /// Clone this token to request cooperative cancellation from another thread.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.control.cancellation.clone()
    }

    fn fresh_environment() -> Environment {
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
        environment
    }

    /// Execute an entry file, retaining its path for subsequent interactive input.
    /// Explicit entry execution is not cached, but participates in cycle detection.
    pub fn run_file(&mut self, path: impl AsRef<Path>) -> Result<(), RuntimeError> {
        self.control.begin_run()?;
        let path = self.modules.entry_path(path.as_ref())?;
        let source = ModuleLoader::read(&path)?;
        let id = ModuleId::File(path.clone());
        self.modules.begin(&id)?;
        self.environment.source_file = Some(path.clone());
        let result = self.run_source_in(&source, Some(&path));
        self.modules.end(&id);
        result
    }

    fn import(&mut self, specifier: &str) -> Result<Value, RuntimeError> {
        let result = self.load_module(specifier);
        result.map_err(|error| {
            let origin = self
                .environment
                .source_file
                .as_deref()
                .map_or_else(|| "<source>".to_owned(), |path| path.display().to_string());
            error.context(
                format!("import '{specifier}' from '{origin}'"),
                self.current_location.clone(),
            )
        })
    }

    fn load_module(&mut self, specifier: &str) -> Result<Value, RuntimeError> {
        self.control.check_cancelled()?;
        let id = self
            .modules
            .resolve(specifier, self.environment.source_file.as_deref())?;
        if let Some(module) = self.modules.cached(&id)? {
            return Ok(module);
        }
        self.control.enter_import()?;
        let result = self.load_uncached_module(&id);
        self.control.leave_import();
        if let Ok(module) = &result {
            self.modules.cache(id, module.clone());
        }
        result
    }

    fn load_uncached_module(&mut self, id: &ModuleId) -> Result<Value, RuntimeError> {
        self.modules.begin(id)?;
        let result = match id {
            ModuleId::Builtin(name) => crate::stdlib::module(name)
                .ok_or_else(|| RuntimeError::new(format!("unknown built-in module '{name}'"))),
            ModuleId::File(path) => self.execute_module(path),
        };
        // Clear in-progress state even after read, parse, or execution errors.
        self.modules.end(id);
        result
    }

    fn execute_module(&mut self, path: &Path) -> Result<Value, RuntimeError> {
        let source = ModuleLoader::read(path)?;
        let mut environment = Self::fresh_environment();
        environment.source_file = Some(path.to_owned());
        let exports = Rc::new(std::cell::RefCell::new(HashMap::new()));
        environment.define(
            "exports".into(),
            Value::Object(exports.clone()),
            true,
            Some("Object".into()),
            false,
        )?;
        let caller = std::mem::replace(&mut self.environment, environment);
        let result = self.run_source_in(&source, Some(path));
        self.environment = caller;
        result?;
        Ok(Value::Module(Rc::new(ModuleValue {
            name: path.display().to_string(),
            exports,
        }))
        .tracked())
    }

    pub fn run_source(&mut self, source: &str) -> Result<(), RuntimeError> {
        self.control.begin_run()?;
        self.run_source_in(source, None)
    }

    fn run_source_in(&mut self, source: &str, file: Option<&Path>) -> Result<(), RuntimeError> {
        self.control.check_cancelled()?;
        let program = parse_with_limits(source, self.control.limits.parse).map_err(|error| {
            let kind = error
                .limit
                .map_or(RuntimeErrorKind::Language, RuntimeErrorKind::LimitExceeded);
            RuntimeError {
                kind,
                ..RuntimeError::new(error.message).at(file, Some(error.span))
            }
        })?;
        self.control.check_cancelled()?;

        let previous = std::mem::replace(
            &mut self.environment.diagnostic_file,
            file.map(Path::to_owned),
        );
        let result = self.execute_program(&program);
        self.environment.diagnostic_file = previous;
        result
    }

    pub fn run_program(&mut self, program: &Program) -> Result<(), RuntimeError> {
        self.control.begin_run()?;
        // Host-created ASTs must be checked before function cloning/evaluation.
        for statement in &program.statements {
            check_depth(
                Node::Statement(statement),
                self.control.limits.parse.max_ast_depth,
            )
            .map_err(|span| {
                RuntimeError::limit(LimitKind::AstDepth, self.control.limits.parse.max_ast_depth)
                    .at(self.environment.diagnostic_file.as_deref(), span)
            })?;
        }
        self.execute_program(program)
    }

    fn execute_program(&mut self, program: &Program) -> Result<(), RuntimeError> {
        let result = self.execute_statements(&program.statements);
        crate::runtime::collect_garbage();
        match result? {
            ControlFlow::Continue(()) => self.control.check_cancelled(),
            ControlFlow::Break(jump) => Err(Self::unexpected_jump(jump)),
        }
    }

    pub fn get_global(&self, name: &str) -> Option<Value> {
        self.environment.get_global(name)
    }

    fn execute(&mut self, statement: &Stmt) -> ExecutionResult {
        let Stmt::Located { span, .. } = statement else {
            let result = self.execute_unlocated(statement);
            crate::runtime::maybe_collect();
            return result;
        };
        let location = SourceLocation {
            file: self.environment.diagnostic_file.clone(),
            span: Some(*span),
        };
        let previous = self.current_location.replace(location.clone());
        let result = self
            .execute_unlocated(statement.unlocated())
            .map_err(|error| error.at(location.file.as_deref(), location.span));
        self.current_location = previous;
        crate::runtime::maybe_collect();
        result
    }

    fn execute_unlocated(&mut self, statement: &Stmt) -> ExecutionResult {
        self.control.enter_evaluation()?;
        let result = self.execute_kind(statement);
        self.control.leave_evaluation();
        result
    }

    fn execute_kind(&mut self, statement: &Stmt) -> ExecutionResult {
        match statement {
            Stmt::Located { .. } => unreachable!("source metadata handled by execute"),
            Stmt::Import { specifier, name } => {
                let module = self.import(specifier)?;
                self.environment
                    .define(name.clone(), module, false, None, false)?;
            }
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

            Stmt::TryCatch {
                body,
                error_name,
                catch_body,
            } => {
                // Only language errors enter the handler; return/break/continue
                // keep their normal targets. The try scope is already restored.
                let error = match self.execute_block(body) {
                    Ok(flow) => return Ok(flow),
                    Err(error) if error.is_catchable() => error,
                    Err(interruption) => return Err(interruption),
                };
                self.environment.begin_scope();
                let result = self
                    .environment
                    .define(error_name.clone(), error.to_value(), false, None, false)
                    .and_then(|()| self.execute_statements(catch_body));
                self.environment.end_scope();
                // Handler failures propagate outward, never back into this catch.
                return result;
            }

            Stmt::While { condition, body } => {
                while self.evaluate(condition)?.is_truthy() {
                    self.control.step()?;
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
        crate::runtime::maybe_collect();
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
            None => Value::Integer(1),
        };
        use crate::runtime::numeric;
        let (mut counter, limit, step) = if matches!(
            (&start, &limit, &step),
            (Value::Integer(_), Value::Integer(_), Value::Integer(_))
        ) {
            (start, limit, step)
        } else {
            (
                Value::Number(numeric::exact_float(&start, "for loop start")?),
                Value::Number(numeric::exact_float(&limit, "for loop limit")?),
                Value::Number(numeric::exact_float(&step, "for loop step")?),
            )
        };
        if numeric::compare(&step, &Value::Integer(0)) == Some(std::cmp::Ordering::Equal) {
            return Err(RuntimeError::new("for loop step must not be zero"));
        }
        let ascending = numeric::compare(&step, &Value::Integer(0)).is_some_and(|o| o.is_gt());
        while numeric::compare(&counter, &limit)
            .is_some_and(|o| if ascending { !o.is_gt() } else { !o.is_lt() })
        {
            self.control.step()?;
            // A new binding per iteration keeps escaped closures independent.
            self.environment.begin_scope();
            let result = self
                .environment
                .define(name.to_owned(), counter.clone(), false, None, false)
                .and_then(|()| self.execute_statements(body));
            self.environment.end_scope();
            crate::runtime::maybe_collect();
            match result? {
                ControlFlow::Break(Jump::Break) => break,
                flow @ ControlFlow::Break(Jump::Return(_)) => return Ok(flow),
                ControlFlow::Break(Jump::Continue) | ControlFlow::Continue(()) => {}
            }

            if counter == limit {
                break;
            }
            // An overflow while advancing is necessarily beyond the finite limit.
            let next = match (&counter, &step) {
                (Value::Integer(a), Value::Integer(b)) => match a.checked_add(*b) {
                    Some(next) => Value::Integer(next),
                    None => break,
                },
                (Value::Number(a), Value::Number(b)) => {
                    let next = a + b;
                    if !next.is_finite() {
                        break;
                    }
                    Value::Number(next)
                }
                _ => unreachable!("loop representations were normalized"),
            };
            if next == counter {
                return Err(RuntimeError::new(
                    "for loop step does not advance the counter",
                ));
            }
            counter = next;
        }
        Ok(ControlFlow::Continue(()))
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
        .tracked()
    }

    fn evaluate(&mut self, expression: &Expr) -> Result<Value, RuntimeError> {
        self.control.enter_evaluation()?;
        let result = self.evaluate_inner(expression);
        self.control.leave_evaluation();
        result
    }

    fn evaluate_inner(&mut self, expression: &Expr) -> Result<Value, RuntimeError> {
        match expression {
            Expr::Function(function) => Ok(self.make_function(None, function)),
            Expr::Nil => Ok(Value::Nil),
            Expr::Boolean(value) => Ok(Value::Boolean(*value)),
            Expr::Number(value) => crate::runtime::numeric::parse(value),
            Expr::String(value) => Ok(Value::String(value.clone())),
            Expr::Identifier(name) => self.environment.get(name),

            Expr::Unary { op, operand } => {
                // Permit the signed minimum literal without first overflowing
                // its positive magnitude. Parentheses preserve this AST shape.
                if matches!(op, UnaryOp::Negate)
                    && matches!(operand.as_ref(), Expr::Number(text) if text.trim_start_matches('0') == "9223372036854775808")
                {
                    self.control.enter_evaluation()?;
                    self.control.leave_evaluation();
                    return Ok(Value::Integer(i64::MIN));
                }
                let value = self.evaluate(operand)?;

                match op {
                    UnaryOp::Negate => crate::runtime::numeric::negate(&value),
                    UnaryOp::Not => Ok(Value::Boolean(!value.is_truthy())),
                    UnaryOp::Length => match value {
                        Value::String(text) => {
                            Ok(crate::runtime::numeric::count(text.chars().count()))
                        }
                        Value::List(items) => Ok(crate::runtime::numeric::count(
                            read_collection(&items, "operator '#'")?.len(),
                        )),
                        Value::Object(properties) => Ok(crate::runtime::numeric::count(
                            read_collection(&properties, "operator '#'")?.len(),
                        )),
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
                crate::stdlib::check_arity(
                    function.name.as_deref().unwrap_or("<anonymous>"),
                    &values,
                    function.definition.parameters.len(),
                    function.definition.parameters.len(),
                )?;

                let result = self.control.enter_call().and_then(|()| {
                    let result = self.call_function(&function, values);
                    self.control.leave_call();
                    result
                });
                let flow = result.map_err(|error| {
                    error.context(
                        format!("in {}()", function.name.as_deref().unwrap_or("<anonymous>")),
                        self.current_location.clone(),
                    )
                })?;
                match flow {
                    ControlFlow::Break(Jump::Return(value)) => Ok(value),
                    ControlFlow::Continue(()) => Ok(Value::Nil),
                    ControlFlow::Break(jump) => Err(Self::unexpected_jump(jump)),
                }
            }
            Value::NativeFunction(function) => crate::stdlib::call(function, &values),
            other => Err(RuntimeError::new(format!(
                "cannot call {}; expected Function",
                other.type_name()
            ))),
        }
    }

    fn call_function(&mut self, function: &UserFunction, values: Vec<Value>) -> ExecutionResult {
        // Only the definition's scopes and a fresh call scope are visible.
        let mut environment = function.closure.clone();
        environment.begin_scope();
        for (name, value) in function.definition.parameters.iter().zip(values) {
            environment.define(name.clone(), value, false, None, false)?;
        }
        let caller = std::mem::replace(&mut self.environment, environment);
        let result = self.execute_statements(&function.definition.body);
        // Restore the caller on returns, language errors, and interruptions.
        self.environment = caller;
        result
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
                crate::runtime::numeric::binary(&left, op, &right)
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
                    (a, b)
                        if crate::runtime::numeric::is_number(a)
                            && crate::runtime::numeric::is_number(b) =>
                    {
                        crate::runtime::numeric::compare(a, b)
                    }
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

    fn read_index(&self, object: Value, index: Value) -> Result<Value, RuntimeError> {
        match object {
            Value::Module(module) => match index {
                Value::String(key) => module.get(&key),
                _ => Err(RuntimeError::new("module index must be a String")),
            },
            Value::List(items) => {
                let index = match crate::runtime::numeric::index(&index, 1, usize::MAX) {
                    Some(index) => index,
                    None => {
                        eprintln!("Warning: invalid list index");
                        return Ok(Value::Nil);
                    }
                };

                let items = read_collection(&items, "list indexing")?;

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

                match read_collection(&properties, "object indexing")?.get(&key) {
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
            Value::Module(module) => module.get(name),
            Value::Object(properties) => {
                match read_collection(&properties, "property access")?.get(name) {
                    Some(value) => Ok(value.clone()),
                    None => {
                        eprintln!("Warning: missing object property '{name}'");
                        Ok(Value::Nil)
                    }
                }
            }

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

                if matches!(object, Value::Module(_)) {
                    return Err(RuntimeError::new("module namespace is read-only"));
                }
                let Value::Object(properties) = object else {
                    return Err(RuntimeError::new("property assignment requires an Object"));
                };

                write_collection(&properties, "property assignment")?.insert(name.clone(), value);
                Ok(())
            }

            Expr::Index { object, index } => {
                let object = self.evaluate(object)?;
                let index = self.evaluate(index)?;

                match object {
                    Value::Module(_) => Err(RuntimeError::new("module namespace is read-only")),
                    Value::List(items) => {
                        if !crate::runtime::numeric::is_number(&index) {
                            return Err(RuntimeError::new("list index must be a number"));
                        }
                        let position =
                            crate::runtime::numeric::index(&index, 1, usize::MAX).ok_or_else(
                                || RuntimeError::new("list index must be a positive integer"),
                            )? - 1;
                        let mut items = write_collection(&items, "list assignment")?;

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

                        write_collection(&properties, "object assignment")?.insert(key, value);
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
