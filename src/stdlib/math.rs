use super::{Arguments, LibraryFunction, NativeResult};
use crate::runtime::Value;

pub(super) const FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::fixed("math.abs", 1, |a| Ok(Value::Number(a.number(0)?.abs()))),
    LibraryFunction::fixed("math.sqrt", 1, sqrt),
    LibraryFunction::fixed("math.floor", 1, |a| Ok(Value::Number(a.number(0)?.floor()))),
    LibraryFunction::fixed("math.ceil", 1, |a| Ok(Value::Number(a.number(0)?.ceil()))),
    LibraryFunction::fixed("math.round", 1, |a| Ok(Value::Number(a.number(0)?.round()))),
    LibraryFunction::range("math.min", 1, usize::MAX, |a| extreme(a, f64::min)),
    LibraryFunction::range("math.max", 1, usize::MAX, |a| extreme(a, f64::max)),
];

fn sqrt(args: &Arguments<'_>) -> NativeResult {
    let number = args.number(0)?;
    if number < 0.0 {
        return Err(args.error(0, "must be nonnegative"));
    }
    Ok(Value::Number(number.sqrt()))
}

fn extreme(args: &Arguments<'_>, combine: fn(f64, f64) -> f64) -> NativeResult {
    let mut result = args.number(0)?;
    for index in 1..args.values.len() {
        result = combine(result, args.number(index)?);
    }
    Ok(Value::Number(result))
}
