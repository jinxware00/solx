use super::{Arguments, LibraryFunction, NativeResult};
use crate::runtime::Value;

pub(super) const LIST_FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::fixed("list.len", 1, |a| {
        Ok(Value::Number(a.list(0)?.borrow().len() as f64))
    }),
    LibraryFunction::fixed("list.append", 2, append),
    LibraryFunction::fixed("list.remove", 2, remove_item),
    LibraryFunction::fixed("list.find", 2, find),
    LibraryFunction::fixed("list.sort", 1, sort),
];

pub(super) const OBJECT_FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::fixed("object.keys", 1, keys),
    LibraryFunction::fixed("object.has", 2, |a| {
        Ok(Value::Boolean(
            a.object(0)?.borrow().contains_key(a.string(1)?),
        ))
    }),
    LibraryFunction::fixed("object.remove", 2, remove_property),
];

fn append(args: &Arguments<'_>) -> NativeResult {
    args.list(0)?.borrow_mut().push(args.values[1].clone());
    Ok(Value::Nil)
}

fn remove_item(args: &Arguments<'_>) -> NativeResult {
    let list = args.list(0)?;
    let len = list.borrow().len();
    if len == 0 {
        return Err(args.error(0, "must not be empty"));
    }
    let index = args.integer(1, 1, len)?;
    Ok(list.borrow_mut().remove(index - 1))
}

fn find(args: &Arguments<'_>) -> NativeResult {
    let items = args.list(0)?.borrow();
    Ok(items
        .iter()
        .position(|item| item == &args.values[1])
        .map_or(Value::Nil, |index| Value::Number((index + 1) as f64)))
}

fn sort(args: &Arguments<'_>) -> NativeResult {
    let mut items = args.list(0)?.borrow_mut();
    let numbers = items
        .iter()
        .all(|v| matches!(v, Value::Number(n) if n.is_finite()));
    let strings = items.iter().all(|v| matches!(v, Value::String(_)));
    if !numbers && !strings {
        return Err(args.error(0, "must contain only finite Numbers or only Strings"));
    }
    items.sort_by(|a, b| match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.partial_cmp(b).expect("validated finite numbers"),
        (Value::String(a), Value::String(b)) => a.cmp(b),
        _ => unreachable!("validated uniform list"),
    });
    Ok(Value::Nil)
}

fn keys(args: &Arguments<'_>) -> NativeResult {
    let properties = args.object(0)?.borrow();
    let mut keys: Vec<_> = properties.keys().cloned().collect();
    keys.sort();
    Ok(Value::list(keys.into_iter().map(Value::String).collect()))
}

fn remove_property(args: &Arguments<'_>) -> NativeResult {
    let properties = args.object(0)?;
    let key = args.string(1)?;
    Ok(properties.borrow_mut().remove(key).unwrap_or(Value::Nil))
}
