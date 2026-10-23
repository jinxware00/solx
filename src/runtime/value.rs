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
    Library(&'static crate::stdlib::LibraryFunction),
}

impl NativeFunction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Str => "str",
            Self::Num => "num",
            Self::Library(function) => function.name(),
        }
    }
}

pub struct ModuleValue {
    pub(crate) name: String,
    pub(crate) exports: Rc<RefCell<HashMap<String, Value>>>,
}

impl fmt::Debug for ModuleValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Module")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl PartialEq for ModuleValue {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl ModuleValue {
    pub(crate) fn get(&self, name: &str) -> Result<Value, super::RuntimeError> {
        super::read_collection(&self.exports, "module export access")?
            .get(name)
            .cloned()
            .ok_or_else(|| {
                super::RuntimeError::new(format!("module '{}' has no export '{name}'", self.name))
            })
    }
}

#[derive(Clone)]
pub enum Value {
    Nil,
    Boolean(bool),
    /// Exact signed integer; exposed to Solx as Number, like the decimal variant.
    Integer(i64),
    Number(f64),
    String(String),
    List(Rc<RefCell<Vec<Value>>>),
    Object(Rc<RefCell<HashMap<String, Value>>>),
    Function(Rc<UserFunction>),
    NativeFunction(NativeFunction),
    Module(Rc<ModuleValue>),
}

impl Value {
    pub fn list(items: Vec<Value>) -> Self {
        Self::List(Rc::new(RefCell::new(items))).tracked()
    }

    pub fn object(properties: HashMap<String, Value>) -> Self {
        Self::Object(Rc::new(RefCell::new(properties))).tracked()
    }

    pub(crate) fn tracked(self) -> Self {
        if let Some(node) = super::gc::Node::from_value(&self) {
            super::gc::register(node);
        }
        self
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Nil => "Nil",
            Self::Boolean(_) => "Boolean",
            Self::Integer(_) | Self::Number(_) => "Number",
            Self::String(_) => "String",
            Self::List(_) => "List",
            Self::Object(_) => "Object",
            Self::Function(_) | Self::NativeFunction(_) => "Function",
            Self::Module(_) => "Module",
        }
    }

    pub fn is_truthy(&self) -> bool {
        !matches!(self, Self::Nil | Self::Boolean(false))
    }

    pub fn to_solx_string(&self) -> String {
        super::collection_ops::format(self, false)
    }

    pub(super) fn scalar_string(&self) -> String {
        match self {
            Self::Nil => "nil".to_owned(),
            Self::Boolean(value) => value.to_string(),
            Self::Integer(value) => value.to_string(),
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
            Self::Module(module) => format!("<module {}>", module.name),
            Self::List(_) | Self::Object(_) => unreachable!("collections use iterative formatting"),
        }
    }

    /// Returns an approximate f64 for host compatibility. Match Integer directly
    /// when exact integer bits matter; Solx arithmetic uses checked conversions.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Integer(value) => Some(*value as f64),
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }
}

/// Structural comparison returns false when a needed collection is mutably
/// borrowed by a Rust host, even for aliases. Retry after releasing the borrow
/// to compare its contents; this does not bypass scalar rules such as NaN.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        super::collection_ops::equal(self, other)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&super::collection_ops::format(self, true))
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

    #[test]
    fn collection_cycles_are_reclaimed_at_a_collection_checkpoint() {
        let value = Value::list(vec![]);
        let Value::List(items) = &value else {
            unreachable!()
        };
        let weak = Rc::downgrade(items);
        items.borrow_mut().push(value.clone());
        drop(value);
        assert!(weak.upgrade().is_some(), "Rc alone retains the self-edge");
        super::super::collect_garbage();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn interpreter_drop_collects_nonrecursive_function_scope_cycles() {
        let mut interpreter = crate::interpreter::Interpreter::new();
        interpreter
            .run_source("global f = fn() return 1 end")
            .unwrap();
        let Value::Function(function) = interpreter.get_global("f").unwrap() else {
            unreachable!()
        };
        let weak = Rc::downgrade(&function);
        drop(function);
        drop(interpreter);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn collection_reclaims_export_function_environment_cycles() {
        let mut environment = Environment::new();
        let exports = Rc::new(RefCell::new(HashMap::new()));
        environment
            .define(
                "exports".into(),
                Value::Object(exports.clone()),
                true,
                None,
                false,
            )
            .unwrap();
        let function = Value::Function(Rc::new(UserFunction {
            name: None,
            definition: Function {
                parameters: vec![],
                body: vec![],
            },
            closure: environment.clone(),
        }));
        exports.borrow_mut().insert("f".into(), function);
        let module = Rc::new(ModuleValue {
            name: "audit".into(),
            exports: exports.clone(),
        });
        let weak_module = Rc::downgrade(&module);
        let weak_exports = Rc::downgrade(&exports);
        drop(environment);
        drop(exports);
        drop(module);
        assert!(
            weak_module.upgrade().is_none(),
            "module namespace itself has no back-edge"
        );
        assert!(
            weak_exports.upgrade().is_some(),
            "Rc alone retains export cycles"
        );
        super::super::collect_garbage();
        assert!(weak_exports.upgrade().is_none());
    }

    #[test]
    fn ownership_audit_acyclic_values_and_uncaptured_functions_are_released() {
        let value = Value::list(vec![Value::list(vec![Value::Number(1.0)])]);
        let Value::List(items) = &value else {
            unreachable!()
        };
        let weak = Rc::downgrade(items);
        drop(value);
        assert!(weak.upgrade().is_none());
        let function = Rc::new(UserFunction {
            name: None,
            definition: Function {
                parameters: vec![],
                body: vec![],
            },
            closure: Environment::new(),
        });
        let weak = Rc::downgrade(&function);
        drop(function);
        assert!(weak.upgrade().is_none());
    }
}
