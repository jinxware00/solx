use super::{Arguments, LibraryFunction, NativeResult};
use crate::runtime::{Value, read_collection, write_collection};

pub(super) const LIST_FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::fixed("list.len", 1, |a| {
        Ok(Value::Number(
            read_collection(a.list(0)?, "list.len()")?.len() as f64,
        ))
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
            read_collection(a.object(0)?, "object.has()")?.contains_key(a.string(1)?),
        ))
    }),
    LibraryFunction::fixed("object.remove", 2, remove_property),
];

fn append(args: &Arguments<'_>) -> NativeResult {
    write_collection(args.list(0)?, "list.append()")?.push(args.values[1].clone());
    Ok(Value::Nil)
}

fn remove_item(args: &Arguments<'_>) -> NativeResult {
    let list = args.list(0)?;
    let len = read_collection(list, "list.remove()")?.len();
    if len == 0 {
        return Err(args.error(0, "must not be empty"));
    }
    let index = args.integer(1, 1, len)?;
    Ok(write_collection(list, "list.remove()")?.remove(index - 1))
}

fn find(args: &Arguments<'_>) -> NativeResult {
    let items = read_collection(args.list(0)?, "list.find()")?;
    Ok(items
        .iter()
        .position(|item| item == &args.values[1])
        .map_or(Value::Nil, |index| Value::Number((index + 1) as f64)))
}

fn sort(args: &Arguments<'_>) -> NativeResult {
    let mut items = write_collection(args.list(0)?, "list.sort()")?;
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
    let properties = read_collection(args.object(0)?, "object.keys()")?;
    let mut keys: Vec<_> = properties.keys().cloned().collect();
    keys.sort();
    Ok(Value::list(keys.into_iter().map(Value::String).collect()))
}

fn remove_property(args: &Arguments<'_>) -> NativeResult {
    let properties = args.object(0)?;
    let key = args.string(1)?;
    Ok(write_collection(properties, "object.remove()")?
        .remove(key)
        .unwrap_or(Value::Nil))
}
