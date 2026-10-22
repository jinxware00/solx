use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::parser::ast::Function;

use super::environment::Environment;

pub struct UserFunction {
    pub(crate) name: Option<String>,
    pub(crate) definition: Function,
    pub(crate) closure: Environment,
}

impl fmt::Debug for UserFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The captured environment can contain this function itself.
        f.debug_struct("UserFunction")
            .field("name", &self.name)
            .field("parameters", &self.definition.parameters)
            .finish_non_exhaustive()
    }
}

impl PartialEq for UserFunction {
    fn eq(&self, other: &Self) -> bool {
        // Functions compare by identity, never by their captured environments.
        std::ptr::eq(self, other)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeFunction {
    Print,
    Str,
    Num,
}

impl NativeFunction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Str => "str",
            Self::Num => "num",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Boolean(bool),
    Number(f64),
    String(String),
    List(Rc<RefCell<Vec<Value>>>),
    Object(Rc<RefCell<HashMap<String, Value>>>),
    Function(Rc<UserFunction>),
    NativeFunction(NativeFunction),
}

impl Value {
    pub fn list(items: Vec<Value>) -> Self {
        Self::List(Rc::new(RefCell::new(items)))
    }

    pub fn object(properties: HashMap<String, Value>) -> Self {
        Self::Object(Rc::new(RefCell::new(properties)))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Nil => "Nil",
            Self::Boolean(_) => "Boolean",
            Self::Number(_) => "Number",
            Self::String(_) => "String",
            Self::List(_) => "List",
            Self::Object(_) => "Object",
            Self::Function(_) | Self::NativeFunction(_) => "Function",
        }
    }

    pub fn is_truthy(&self) -> bool {
        !matches!(self, Self::Nil | Self::Boolean(false))
    }

    pub fn to_solx_string(&self) -> String {
        match self {
            Self::Nil => "nil".to_owned(),
            Self::Boolean(value) => value.to_string(),
            Self::Number(value) => {
                if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e16 {
                    format!("{value:.0}")
                } else {
                    value.to_string()
                }
            }
            Self::String(value) => value.clone(),
            Self::Function(function) => match &function.name {
                Some(name) => format!("<function {name}>"),
                None => "<function>".to_owned(),
            },
            Self::NativeFunction(function) => format!("<function {}>", function.name()),
            Self::List(items) => {
                let items = items.borrow();
                let contents = items
                    .iter()
                    .map(Self::to_solx_string)
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("[{contents}]")
            }
            Self::Object(properties) => {
                let properties = properties.borrow();
                let mut entries = properties
                    .iter()
                    .map(|(key, value)| format!("{key} = {}", value.to_solx_string()))
                    .collect::<Vec<_>>();

                entries.sort();
                format!("{{{}}}", entries.join(", "))
            }
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_solx_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_solx_truthiness() {
        assert!(!Value::Nil.is_truthy());
        assert!(!Value::Boolean(false).is_truthy());
        assert!(Value::Boolean(true).is_truthy());
        assert!(Value::Number(0.0).is_truthy());
        assert!(Value::String(String::new()).is_truthy());
        assert!(Value::list(Vec::new()).is_truthy());
    }

    #[test]
    fn formats_basic_values() {
        assert_eq!(Value::Number(42.0).to_string(), "42");
        assert_eq!(Value::Boolean(true).to_string(), "true");
        assert_eq!(Value::Nil.to_string(), "nil");
    }
}
