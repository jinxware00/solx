use std::time::{SystemTime, UNIX_EPOCH};

use super::{Arguments, LibraryFunction, NativeResult};
use crate::{
    interpreter::Interpreter,
    runtime::{RuntimeError, Value},
};

pub(super) const FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::with_interpreter("time.unix_ms", 0, unix_ms),
    LibraryFunction::with_interpreter("time.elapsed_ms", 0, elapsed_ms),
];

fn millis(value: u128, name: &str) -> NativeResult {
    i64::try_from(value).map(Value::Integer).map_err(|_| {
        RuntimeError::new(format!(
            "{name}() exceeds the signed 64-bit millisecond range"
        ))
    })
}

fn unix_ms(interpreter: &mut Interpreter, args: &Arguments<'_>) -> NativeResult {
    interpreter.host.clock(args.name)?;
    interpreter.native_checkpoint()?;
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RuntimeError::new("time.unix_ms() system clock is before the Unix epoch"))?;
    millis(duration.as_millis(), args.name)
}

fn elapsed_ms(interpreter: &mut Interpreter, args: &Arguments<'_>) -> NativeResult {
    let start = interpreter.host.clock(args.name)?;
    interpreter.native_checkpoint()?;
    millis(start.elapsed().as_millis(), args.name)
}
