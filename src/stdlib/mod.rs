//! Rust-backed standard modules. Only explicitly imported namespaces are built.

mod collections;
mod core;
mod json;
mod math;
mod strings;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::interpreter::Interpreter;
use crate::runtime::value::{ModuleValue, NativeFunction};
use crate::runtime::{RuntimeError, Value};

type NativeResult = Result<Value, RuntimeError>;
type List = Rc<RefCell<Vec<Value>>>;
type Object = Rc<RefCell<HashMap<String, Value>>>;

/// A native function. Identity is its unique registered qualified name.
#[derive(Debug, Clone, Copy)]
pub struct LibraryFunction {
    name: &'static str,
    min: usize,
    max: usize,
    implementation: Implementation,
}

#[derive(Debug, Clone, Copy)]
enum Implementation {
    Stateless(fn(&Arguments<'_>) -> NativeResult),
    WithInterpreter(fn(&mut Interpreter, &Arguments<'_>) -> NativeResult),
}

impl PartialEq for LibraryFunction {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for LibraryFunction {}

impl LibraryFunction {
    const fn fixed(
        name: &'static str,
        count: usize,
        implementation: fn(&Arguments<'_>) -> NativeResult,
    ) -> Self {
        Self::range(name, count, count, implementation)
    }

    const fn range(
        name: &'static str,
        min: usize,
        max: usize,
        implementation: fn(&Arguments<'_>) -> NativeResult,
    ) -> Self {
        Self {
            name,
            min,
            max,
            implementation: Implementation::Stateless(implementation),
        }
    }

    const fn with_interpreter(
        name: &'static str,
        count: usize,
        implementation: fn(&mut Interpreter, &Arguments<'_>) -> NativeResult,
    ) -> Self {
        Self {
            name,
            min: count,
            max: count,
            implementation: Implementation::WithInterpreter(implementation),
        }
    }

    pub fn name(self) -> &'static str {
        self.name
    }

    fn invoke(self, interpreter: &mut Interpreter, values: &[Value]) -> NativeResult {
        check_arity(self.name, values, self.min, self.max)?;
        let args = Arguments {
            name: self.name,
            values,
        };
        match self.implementation {
            Implementation::Stateless(function) => function(&args),
            Implementation::WithInterpreter(function) => function(interpreter, &args),
        }
    }
}

struct Arguments<'a> {
    name: &'static str,
    values: &'a [Value],
}

impl<'a> Arguments<'a> {
    fn error(&self, index: usize, requirement: &str) -> RuntimeError {
        RuntimeError::new(format!(
            "{}() argument {} {requirement}",
            self.name,
            index + 1
        ))
    }

    fn wrong_type(&self, index: usize, expected: &str) -> RuntimeError {
        self.error(
            index,
            &format!(
                "requires {expected}, got {}",
                self.values[index].type_name()
            ),
        )
    }

    fn string(&self, index: usize) -> Result<&'a str, RuntimeError> {
        match &self.values[index] {
            Value::String(value) => Ok(value),
            _ => Err(self.wrong_type(index, "String")),
        }
    }

    fn function(&self, index: usize) -> Result<&'a Value, RuntimeError> {
        match &self.values[index] {
            value @ (Value::Function(_) | Value::NativeFunction(_)) => Ok(value),
            _ => Err(self.wrong_type(index, "Function")),
        }
    }

    fn number(&self, index: usize) -> Result<f64, RuntimeError> {
        if !crate::runtime::numeric::is_number(&self.values[index]) {
            return Err(self.wrong_type(index, "Number"));
        }
        if matches!(self.values[index], Value::Number(number) if !number.is_finite()) {
            return Err(self.error(index, "must be finite"));
        }
        crate::runtime::numeric::exact_float(&self.values[index], self.name)
    }

    fn integer(&self, index: usize, min: usize, max: usize) -> Result<usize, RuntimeError> {
        if !crate::runtime::numeric::is_number(&self.values[index]) {
            return Err(self.wrong_type(index, "Number"));
        }
        if matches!(self.values[index], Value::Number(number) if !number.is_finite()) {
            return Err(self.error(index, "must be finite"));
        }
        crate::runtime::numeric::index(&self.values[index], min, max)
            .ok_or_else(|| self.error(index, &format!("must be an integer in {min}..{max}")))
    }

    fn list(&self, index: usize) -> Result<&'a List, RuntimeError> {
        match &self.values[index] {
            Value::List(items) => Ok(items),
            _ => Err(self.wrong_type(index, "List")),
        }
    }

    fn object(&self, index: usize) -> Result<&'a Object, RuntimeError> {
        match &self.values[index] {
            Value::Object(properties) => Ok(properties),
            _ => Err(self.wrong_type(index, "Object")),
        }
    }
}

pub(crate) fn check_arity(
    name: &str,
    values: &[Value],
    min: usize,
    max: usize,
) -> Result<(), RuntimeError> {
    if (min..=max).contains(&values.len()) {
        return Ok(());
    }
    let expected = if min == max {
        min.to_string()
    } else if max == usize::MAX {
        format!("at least {min}")
    } else {
        format!("{min} to {max}")
    };
    Err(RuntimeError::new(format!(
        "{name}() expects {expected} argument(s), got {}",
        values.len()
    )))
}

pub(crate) fn call(
    interpreter: &mut Interpreter,
    function: NativeFunction,
    values: &[Value],
) -> NativeResult {
    let name = function.name();
    match function {
        NativeFunction::Library(function) => function.invoke(interpreter, values),
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
            check_arity(name, values, 1, 1)?;
            Ok(Value::String(values[0].to_solx_string()))
        }
        NativeFunction::Num => {
            check_arity(name, values, 1, 1)?;
            match &values[0] {
                Value::Integer(number) => Ok(Value::Integer(*number)),
                Value::Number(number) => {
                    crate::runtime::numeric::finite(*number).map(Value::Number)
                }
                Value::String(text) => crate::runtime::numeric::parse(text),
                other => Err(RuntimeError::new(format!(
                    "num() cannot convert {} to Number",
                    other.type_name()
                ))),
            }
        }
    }
}

pub(crate) fn module(name: &str) -> Option<Value> {
    let functions = match name {
        "core" => core::FUNCTIONS,
        "json" => json::FUNCTIONS,
        "math" => math::FUNCTIONS,
        "string" => strings::FUNCTIONS,
        "list" => collections::LIST_FUNCTIONS,
        "object" => collections::OBJECT_FUNCTIONS,
        _ => return None,
    };
    let mut exports: HashMap<_, _> = functions
        .iter()
        .map(|function| {
            let key = function
                .name
                .rsplit('.')
                .next()
                .expect("qualified native name");
            (
                key.to_owned(),
                Value::NativeFunction(NativeFunction::Library(function)),
            )
        })
        .collect();
    if name == "math" {
        exports.insert("pi".into(), Value::Number(std::f64::consts::PI));
    }
    Some(
        Value::Module(Rc::new(ModuleValue {
            name: name.to_owned(),
            exports: Rc::new(RefCell::new(exports)),
        }))
        .tracked(),
    )
}
