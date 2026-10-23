//! Iterative graph operations. Collection identities are traversal-local, never
//! language-visible. Functions and modules remain opaque identity values.
use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashSet;
use std::rc::Rc;

use super::{RuntimeError, Value};

// Public collection payloads can be borrowed by Rust hosts between interpreter
// calls. Fallible runtime operations must propagate conflicts, not fabricate a
// value or silently skip a mutation. Private environment maps use other rules.
pub(crate) fn read_collection<'a, T>(
    collection: &'a RefCell<T>,
    operation: &str,
) -> Result<Ref<'a, T>, RuntimeError> {
    collection
        .try_borrow()
        .map_err(|_| RuntimeError::new(format!("{operation}: collection is mutably borrowed")))
}

pub(crate) fn write_collection<'a, T>(
    collection: &'a RefCell<T>,
    operation: &str,
) -> Result<RefMut<'a, T>, RuntimeError> {
    collection
        .try_borrow_mut()
        .map_err(|_| RuntimeError::new(format!("{operation}: collection is already borrowed")))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Identity {
    List(usize),
    Object(usize),
}

enum Task {
    Visit(Value),
    List(Identity, usize),
    Object(Identity, Vec<String>),
}

pub(super) fn format(value: &Value, debug: bool) -> String {
    let mut active = HashSet::new();
    let mut tasks = vec![Task::Visit(value.clone())];
    let mut rendered: Vec<String> = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(Value::List(items)) => {
                let id = Identity::List(Rc::as_ptr(&items) as usize);
                if !active.insert(id) {
                    rendered.push("<cycle>".into());
                    continue;
                }
                let Ok(items) = items.try_borrow() else {
                    active.remove(&id);
                    rendered.push("<borrowed>".into());
                    continue;
                };
                tasks.push(Task::List(id, items.len()));
                tasks.extend(items.iter().rev().cloned().map(Task::Visit));
            }
            Task::Visit(Value::Object(properties)) => {
                let id = Identity::Object(Rc::as_ptr(&properties) as usize);
                if !active.insert(id) {
                    rendered.push("<cycle>".into());
                    continue;
                }
                let Ok(properties) = properties.try_borrow() else {
                    active.remove(&id);
                    rendered.push("<borrowed>".into());
                    continue;
                };
                let (keys, values): (Vec<_>, Vec<_>) = properties
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .unzip();
                tasks.push(Task::Object(id, keys));
                tasks.extend(values.into_iter().rev().map(Task::Visit));
            }
            Task::Visit(value) => rendered.push(if debug {
                match value {
                    Value::Nil => "Nil".into(),
                    Value::Boolean(value) => format!("Boolean({value:?})"),
                    Value::Integer(value) => format!("Integer({value})"),
                    Value::Number(value) => format!("Number({value:?})"),
                    Value::String(value) => format!("String({value:?})"),
                    Value::Function(value) => format!("Function({value:?})"),
                    Value::NativeFunction(value) => format!("NativeFunction({value:?})"),
                    Value::Module(value) => format!("Module({value:?})"),
                    Value::List(_) | Value::Object(_) => unreachable!(),
                }
            } else {
                value.scalar_string()
            }),
            Task::List(id, count) => {
                let entries = rendered.split_off(rendered.len() - count);
                rendered.push(format!("[{}]", entries.join(", ")));
                active.remove(&id);
            }
            Task::Object(id, keys) => {
                let values = rendered.split_off(rendered.len() - keys.len());
                let mut entries: Vec<_> = keys
                    .into_iter()
                    .zip(values)
                    .map(|(key, value)| {
                        if debug {
                            format!("{key:?} = {value}")
                        } else {
                            format!("{key} = {value}")
                        }
                    })
                    .collect();
                // Preserve sorting of complete rendered entries, not just keys.
                entries.sort();
                rendered.push(format!("{{{}}}", entries.join(", ")));
                active.remove(&id);
            }
        }
    }
    rendered.pop().expect("one rendered root")
}

pub(super) fn equal(left: &Value, right: &Value) -> bool {
    // A host can hold a mutable collection borrow. PartialEq cannot report a
    // borrow error, so comparisons needing unavailable contents return false,
    // including aliases. Identity alone cannot establish structural equality:
    // even the same collection can contain NaN.
    let mut visited = HashSet::new();
    let mut pending = vec![(left.clone(), right.clone())];
    while let Some((left, right)) = pending.pop() {
        match (left, right) {
            (Value::Nil, Value::Nil) => {}
            (Value::Boolean(a), Value::Boolean(b)) if a == b => {}
            (ref a, ref b)
                if super::numeric::is_number(a)
                    && super::numeric::is_number(b)
                    && super::numeric::compare(a, b) == Some(std::cmp::Ordering::Equal) => {}
            (Value::String(a), Value::String(b)) if a == b => {}
            (Value::Function(a), Value::Function(b)) if Rc::ptr_eq(&a, &b) => {}
            (Value::NativeFunction(a), Value::NativeFunction(b)) if a == b => {}
            (Value::Module(a), Value::Module(b)) if Rc::ptr_eq(&a, &b) => {}
            (Value::List(a), Value::List(b)) => {
                let pair = (
                    Identity::List(Rc::as_ptr(&a) as usize),
                    Identity::List(Rc::as_ptr(&b) as usize),
                );
                if !visited.insert(pair) {
                    continue;
                }
                let (Ok(a), Ok(b)) = (a.try_borrow(), b.try_borrow()) else {
                    return false;
                };
                if a.len() != b.len() {
                    return false;
                }
                pending.extend(a.iter().cloned().zip(b.iter().cloned()));
            }
            (Value::Object(a), Value::Object(b)) => {
                let pair = (
                    Identity::Object(Rc::as_ptr(&a) as usize),
                    Identity::Object(Rc::as_ptr(&b) as usize),
                );
                if !visited.insert(pair) {
                    continue;
                }
                let (Ok(a), Ok(b)) = (a.try_borrow(), b.try_borrow()) else {
                    return false;
                };
                if a.len() != b.len() {
                    return false;
                }
                for (key, value) in a.iter() {
                    let Some(other) = b.get(key) else {
                        return false;
                    };
                    pending.push((value.clone(), other.clone()));
                }
            }
            _ => return false,
        }
    }
    true
}
