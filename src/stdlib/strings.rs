use super::{Arguments, LibraryFunction, NativeResult};
use crate::runtime::{Value, read_collection};

pub(super) const FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::fixed("string.len", 1, |a| {
        Ok(crate::runtime::numeric::count(a.string(0)?.chars().count()))
    }),
    LibraryFunction::fixed("string.upper", 1, |a| {
        Ok(Value::String(a.string(0)?.to_uppercase()))
    }),
    LibraryFunction::fixed("string.lower", 1, |a| {
        Ok(Value::String(a.string(0)?.to_lowercase()))
    }),
    LibraryFunction::fixed("string.trim", 1, |a| {
        Ok(Value::String(a.string(0)?.trim().to_owned()))
    }),
    LibraryFunction::fixed("string.slice", 3, slice),
    LibraryFunction::fixed("string.find", 2, find),
    LibraryFunction::fixed("string.split", 2, split),
    LibraryFunction::fixed("string.join", 2, join),
    LibraryFunction::fixed("string.starts_with", 2, |a| {
        Ok(Value::Boolean(a.string(0)?.starts_with(a.string(1)?)))
    }),
    LibraryFunction::fixed("string.ends_with", 2, |a| {
        Ok(Value::Boolean(a.string(0)?.ends_with(a.string(1)?)))
    }),
];

fn slice(args: &Arguments<'_>) -> NativeResult {
    let text = args.string(0)?;
    let len = text.chars().count();
    let start = args.integer(1, 1, len + 1)? - 1;
    let count = args.integer(2, 0, len - start)?;
    Ok(Value::String(
        text.chars().skip(start).take(count).collect(),
    ))
}

fn find(args: &Arguments<'_>) -> NativeResult {
    let text = args.string(0)?;
    let needle = args.string(1)?;
    Ok(text.find(needle).map_or(Value::Nil, |offset| {
        crate::runtime::numeric::count(text[..offset].chars().count() + 1)
    }))
}

fn split(args: &Arguments<'_>) -> NativeResult {
    let text = args.string(0)?;
    let separator = args.string(1)?;
    if separator.is_empty() {
        return Err(args.error(1, "must not be empty"));
    }
    Ok(Value::list(
        text.split(separator)
            .map(|s| Value::String(s.to_owned()))
            .collect(),
    ))
}

fn join(args: &Arguments<'_>) -> NativeResult {
    let items = read_collection(args.list(0)?, "string.join()")?;
    let separator = args.string(1)?;
    let strings = items
        .iter()
        .enumerate()
        .map(|(index, value)| {
            if let Value::String(text) = value {
                Ok(text.as_str())
            } else {
                Err(args.error(
                    0,
                    &format!(
                        "contains {} at index {}; expected String",
                        value.type_name(),
                        index + 1
                    ),
                ))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::String(strings.join(separator)))
}
