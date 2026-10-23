use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::error::RuntimeError;
use super::value::Value;

#[derive(Debug, Clone)]
pub(super) struct Binding {
    pub(super) value: Value,
    constant: bool,
    type_annotation: Option<String>,
}

pub(super) type Scope = Rc<RefCell<HashMap<String, Binding>>>;

fn new_scope() -> Scope {
    let scope = Scope::default();
    super::gc::register(super::gc::Node::Scope(scope.clone()));
    scope
}

/// A lexical scope chain. Clones share bindings, but have independent scope stacks.
#[derive(Debug, Clone)]
pub struct Environment {
    scopes: Vec<Scope>,
    globals: Scope,
    // Definition context travels with closures; it is never a Solx-visible value.
    pub(crate) source_file: Option<std::path::PathBuf>,
    // Separate from the import base: REPL/input buffers are not entry-file lines.
    pub(crate) diagnostic_file: Option<std::path::PathBuf>,
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment {
    pub fn new() -> Self {
        Self {
            scopes: vec![new_scope()],
            globals: new_scope(),
            source_file: None,
            diagnostic_file: None,
        }
    }

    pub fn begin_scope(&mut self) {
        self.scopes.push(new_scope());
    }

    pub fn end_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub(super) fn collection_scopes(&self) -> Vec<super::gc::Node> {
        self.scopes
            .iter()
            .chain(std::iter::once(&self.globals))
            .cloned()
            .map(super::gc::Node::Scope)
            .collect()
    }

    /// Release this owner's references, without clearing any captured scope.
    pub(crate) fn release_roots(&mut self) {
        self.scopes.clear();
        self.globals = Scope::default();
    }

    pub fn define(
        &mut self,
        name: String,
        value: Value,
        constant: bool,
        type_annotation: Option<String>,
        global: bool,
    ) -> Result<(), RuntimeError> {
        Self::check_type(&name, &value, type_annotation.as_deref())?;

        let target = if global {
            &self.globals
        } else {
            self.scopes.last().expect("environment always has a scope")
        };
        let mut target = target.borrow_mut();

        if let Some(existing) = target.get(&name)
            && existing.constant
        {
            return Err(RuntimeError::new(format!(
                "cannot redeclare constant '{name}'"
            )));
        }

        target.insert(
            name,
            Binding {
                value: value.tracked(),
                constant,
                type_annotation,
            },
        );

        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<Value, RuntimeError> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.borrow().get(name) {
                return Ok(binding.value.clone());
            }
        }

        if let Some(binding) = self.globals.borrow().get(name) {
            return Ok(binding.value.clone());
        }

        eprintln!("Warning: undeclared variable '{name}'");

        Err(RuntimeError::new(format!("undeclared variable '{name}'")))
    }

    pub fn assign(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        for scope in self
            .scopes
            .iter()
            .rev()
            .chain(std::iter::once(&self.globals))
        {
            let mut scope = scope.borrow_mut();
            if let Some(binding) = scope.get_mut(name) {
                if binding.constant {
                    return Err(RuntimeError::new(format!(
                        "cannot reassign constant '{name}'"
                    )));
                }

                Self::check_type(name, &value, binding.type_annotation.as_deref())?;

                binding.value = value.tracked();

                return Ok(());
            }
        }

        eprintln!("Warning: undeclared variable '{name}'");
        Err(RuntimeError::new(format!(
            "cannot assign to undeclared variable '{name}'"
        )))
    }

    pub fn get_global(&self, name: &str) -> Option<Value> {
        self.globals
            .borrow()
            .get(name)
            .map(|binding| binding.value.clone())
    }

    fn check_type(name: &str, value: &Value, annotation: Option<&str>) -> Result<(), RuntimeError> {
        let Some(expected) = annotation else {
            return Ok(());
        };

        let compatible = match expected {
            "Number" => matches!(value, Value::Integer(_) | Value::Number(_)),
            "String" => matches!(value, Value::String(_)),
            "Boolean" => matches!(value, Value::Boolean(_)),
            "List" => matches!(value, Value::List(_)),
            "Object" => matches!(value, Value::Object(_)),
            "Module" => matches!(value, Value::Module(_)),
            "Function" => matches!(value, Value::Function(_) | Value::NativeFunction(_)),
            "Nil" => matches!(value, Value::Nil),
            _ => {
                return Err(RuntimeError::new(format!(
                    "unknown type '{expected}' in annotation for '{name}'"
                )));
            }
        };

        if compatible {
            Ok(())
        } else {
            Err(RuntimeError::new(format!(
                "type mismatch for '{name}': expected {expected}, got {}",
                value.type_name()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_environment_has_a_writable_scope() {
        let mut env = Environment::default();
        env.define("value".into(), Value::Number(1.0), false, None, false)
            .unwrap();
        assert_eq!(env.get("value").unwrap(), Value::Number(1.0));
    }

    #[test]
    fn captured_scope_survives_popping_and_shares_assignments() {
        let mut env = Environment::new();
        env.begin_scope();
        env.define("value".into(), Value::Number(1.0), false, None, false)
            .unwrap();
        let mut captured = env.clone();
        env.assign("value", Value::Number(2.0)).unwrap();
        env.end_scope();
        assert!(env.get("value").is_err());
        assert_eq!(captured.get("value").unwrap(), Value::Number(2.0));
        captured.assign("value", Value::Number(3.0)).unwrap();
        assert_eq!(captured.get("value").unwrap(), Value::Number(3.0));
    }

    #[test]
    fn declares_reads_and_reassigns_variables() {
        let mut env = Environment::new();

        env.define("score".into(), Value::Number(50.0), false, None, false)
            .unwrap();

        env.assign("score", Value::Number(100.0)).unwrap();

        assert_eq!(env.get("score").unwrap(), Value::Number(100.0));
    }

    #[test]
    fn nil_assignment_retains_a_writable_binding() {
        let mut env = Environment::new();

        env.define("score".into(), Value::Number(50.0), false, None, false)
            .unwrap();

        env.assign("score", Value::Nil).unwrap();

        assert_eq!(env.get("score").unwrap(), Value::Nil);
        env.assign("score", Value::Number(75.0)).unwrap();
        assert_eq!(env.get("score").unwrap(), Value::Number(75.0));
    }

    #[test]
    fn constants_cannot_be_reassigned() {
        let mut env = Environment::new();

        env.define(
            "pi".into(),
            Value::Number(std::f64::consts::PI),
            true,
            None,
            false,
        )
        .unwrap();

        assert!(env.assign("pi", Value::Number(4.0)).is_err());
    }

    #[test]
    fn annotations_reject_incompatible_values() {
        let mut env = Environment::new();

        env.define(
            "score".into(),
            Value::Number(100.0),
            false,
            Some("Number".into()),
            false,
        )
        .unwrap();

        assert!(env.assign("score", Value::String("hello".into())).is_err());
    }

    #[test]
    fn nested_scope_can_shadow_an_outer_variable() {
        let mut env = Environment::new();

        env.define("score".into(), Value::Number(10.0), false, None, false)
            .unwrap();

        env.begin_scope();

        env.define("score".into(), Value::Number(20.0), false, None, false)
            .unwrap();

        assert_eq!(env.get("score").unwrap(), Value::Number(20.0));

        env.end_scope();

        assert_eq!(env.get("score").unwrap(), Value::Number(10.0));
    }
}
