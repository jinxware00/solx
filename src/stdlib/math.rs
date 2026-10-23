use super::{Arguments, LibraryFunction, NativeResult};
use crate::runtime::{Value, numeric};

pub(super) const FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::fixed("math.abs", 1, abs),
    LibraryFunction::fixed("math.sqrt", 1, sqrt),
    LibraryFunction::fixed("math.floor", 1, |a| round(a, f64::floor)),
    LibraryFunction::fixed("math.ceil", 1, |a| round(a, f64::ceil)),
    LibraryFunction::fixed("math.round", 1, |a| round(a, f64::round)),
    LibraryFunction::range("math.min", 1, usize::MAX, |a| extreme(a, false)),
    LibraryFunction::range("math.max", 1, usize::MAX, |a| extreme(a, true)),
];

fn abs(args: &Arguments<'_>) -> NativeResult {
    if let Value::Integer(integer) = args.values[0] {
        integer
            .checked_abs()
            .map(Value::Integer)
            .ok_or_else(|| args.error(0, "would overflow the integer range"))
    } else {
        Ok(Value::Number(args.number(0)?.abs()))
    }
}

fn round(args: &Arguments<'_>, operation: fn(f64) -> f64) -> NativeResult {
    if let Value::Integer(integer) = args.values[0] {
        Ok(Value::Integer(integer))
    } else {
        Ok(Value::Number(operation(args.number(0)?)))
    }
}

fn sqrt(args: &Arguments<'_>) -> NativeResult {
    let number = args.number(0)?;
    if number < 0.0 {
        return Err(args.error(0, "must be nonnegative"));
    }
    Ok(Value::Number(number.sqrt()))
}

fn extreme(args: &Arguments<'_>, maximum: bool) -> NativeResult {
    let mut result = &args.values[0];
    for (index, value) in args.values.iter().enumerate() {
        match value {
            Value::Integer(_) => {}
            Value::Number(number) if number.is_finite() => {}
            Value::Number(_) => return Err(args.error(index, "must be finite")),
            _ => return Err(args.wrong_type(index, "Number")),
        }
        let ordering = numeric::compare(value, result);
        if ordering.is_some_and(|o| if maximum { o.is_gt() } else { o.is_lt() }) {
            result = value;
        }
    }
    Ok(result.clone())
}
