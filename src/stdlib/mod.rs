//! Rust-backed standard modules. Only explicitly imported namespaces are built.

mod collections;
mod core;
mod math;
mod strings;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::runtime::value::{ModuleValue, NativeFunction};
use crate::runtime::{RuntimeError, Value};

type NativeResult = Result<Value, RuntimeError>;
type List = Rc<RefCell<Vec<Value>>>;
type Object = Rc<RefCell<HashMap<String, Value>>>;

/// A stateless native function. Identity is its unique registered qualified name.
#[derive(Debug, Clone, Copy)]
pub struct LibraryFunction {
    name: &'static str,
    min: usize,
    max: usize,
    implementation: fn(&Arguments<'_>) -> NativeResult,
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
            implementation,
        }
    }

    pub fn name(self) -> &'static str {
        self.name
    }

    fn invoke(self, values: &[Value]) -> NativeResult {
        check_arity(self.name, values, self.min, self.max)?;
        (self.implementation)(&Arguments {
            name: self.name,
            values,
        })
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

    fn number(&self, index: usize) -> Result<f64, RuntimeError> {
        let Value::Number(number) = self.values[index] else {
            return Err(self.wrong_type(index, "Number"));
        };
        if !number.is_finite() {
            return Err(self.error(index, "must be finite"));
        }
        Ok(number)
    }

    fn integer(&self, index: usize, min: usize, max: usize) -> Result<usize, RuntimeError> {
        let number = self.number(index)?;
        if number.fract() != 0.0 || number < min as f64 || number > max as f64 {
            return Err(self.error(index, &format!("must be an integer in {min}..{max}")));
        }
        Ok(number as usize)
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

pub(crate) fn call(function: NativeFunction, values: &[Value]) -> NativeResult {
    let name = function.name();
    match function {
        NativeFunction::Library(function) => function.invoke(values),
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
                Value::Number(number) => Ok(Value::Number(*number)),
                Value::String(text) => {
                    text.trim().parse::<f64>().map(Value::Number).map_err(|_| {
                        RuntimeError::new(format!("cannot convert '{text}' to Number"))
                    })
                }
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
    Some(Value::Module(Rc::new(ModuleValue {
        name: name.to_owned(),
        exports: Rc::new(RefCell::new(exports)),
    })))
}
