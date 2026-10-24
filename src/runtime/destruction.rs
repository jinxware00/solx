//! Drain uniquely owned runtime graphs without following Rc drop chains on the
//! Rust stack. Shared nodes retain their contents and cycles remain GC's job.
use std::rc::Rc;

use super::Value;
use super::gc::Node;

/// Child Values relinquish their Rc only after the worklist has an owner. Their
/// Drop implementation therefore observes sharing and does not recurse.
fn defer(values: impl IntoIterator<Item = Value>, pending: &mut Vec<Node>) {
    for value in values {
        if let Some(node) = Node::from_value(&value) {
            pending.push(node);
        }
    }
}

pub(super) fn drain(first: Node) {
    let mut pending = vec![first];
    while let Some(node) = pending.pop() {
        match node {
            Node::List(items) => {
                if let Ok(items) = Rc::try_unwrap(items) {
                    defer(items.into_inner(), &mut pending);
                }
            }
            Node::Object(properties) => {
                if let Ok(properties) = Rc::try_unwrap(properties) {
                    defer(properties.into_inner().into_values(), &mut pending);
                }
            }
            Node::Scope(scope) => {
                if let Ok(scope) = Rc::try_unwrap(scope) {
                    defer(
                        scope
                            .into_inner()
                            .into_values()
                            .map(|binding| binding.value),
                        &mut pending,
                    );
                }
            }
            Node::Function(function) => {
                if let Ok(function) = Rc::try_unwrap(function) {
                    pending.extend(function.closure.into_collection_scopes());
                }
            }
            Node::Module(module) => {
                if let Ok(module) = Rc::try_unwrap(module) {
                    pending.push(Node::Object(module.exports));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        parser::ast::Function,
        runtime::{
            Environment,
            value::{ModuleValue, UserFunction},
        },
    };
    use std::{
        cell::RefCell,
        collections::HashMap,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    #[test]
    fn deep_function_module_and_scope_destruction_in_subprocess() {
        const NAME: &str =
            "runtime::destruction::tests::deep_function_module_and_scope_destruction_in_subprocess";
        if std::env::var("SOLX_DESTRUCTION_CHILD").as_deref() == Ok(NAME) {
            std::thread::Builder::new()
                .stack_size(256 * 1024)
                .spawn(|| {
                    let mut value = Value::Nil;
                    let mut functions = Vec::new();
                    let mut modules = Vec::new();
                    for _ in 0..20_000 {
                        let exports =
                            Rc::new(RefCell::new(HashMap::from([("child".into(), value)])));
                        let module = Rc::new(ModuleValue {
                            name: "test".into(),
                            exports,
                        });
                        modules.push(Rc::downgrade(&module));
                        let mut closure = Environment::new();
                        closure
                            .define("previous".into(), Value::Module(module), false, None, false)
                            .unwrap();
                        let function = Rc::new(UserFunction {
                            name: None,
                            definition: Function {
                                parameters: vec![],
                                body: vec![],
                            },
                            closure,
                        });
                        functions.push(Rc::downgrade(&function));
                        value = Value::Function(function);
                    }
                    let Value::Function(function) = &value else {
                        unreachable!()
                    };
                    let host = function.clone();
                    drop(value);
                    assert!(functions.iter().all(|weak| weak.strong_count() == 1));
                    drop(host);
                    assert!(functions.iter().all(|weak| weak.strong_count() == 0));
                    assert!(modules.iter().all(|weak| weak.strong_count() == 0));
                    assert_eq!(crate::runtime::collect_garbage().remaining, 0);
                })
                .unwrap()
                .join()
                .unwrap();
            return;
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env("SOLX_DESTRUCTION_CHILD", NAME)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if child.try_wait().unwrap().is_some() {
                let output = child.wait_with_output().unwrap();
                assert!(
                    output.status.success(),
                    "{:?}\n{}\n{}",
                    output.status,
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "child exceeded 30s: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
