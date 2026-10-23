use super::{Arguments, LibraryFunction, NativeResult};
use crate::runtime::{RuntimeError, Value};

pub(super) const FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::fixed("core.type", 1, type_of),
    LibraryFunction::range("core.assert", 1, 2, assert),
    LibraryFunction::fixed("core.error", 1, error),
];

fn type_of(args: &Arguments<'_>) -> NativeResult {
    Ok(Value::String(args.values[0].type_name().to_owned()))
}

fn assert(args: &Arguments<'_>) -> NativeResult {
    let message = if args.values.len() == 2 {
        Some(args.string(1)?)
    } else {
        None
    };
    if args.values[0].is_truthy() {
        return Ok(args.values[0].clone());
    }
    Err(RuntimeError::new(match message {
        Some(message) => format!("assertion failed: {message}"),
        None => "assertion failed".to_owned(),
    }))
}

fn error(args: &Arguments<'_>) -> NativeResult {
    Err(RuntimeError::new(args.string(0)?))
}
