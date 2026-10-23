//! One language-level Number type, with exact i64 integers and finite f64 decimals.
use std::cmp::Ordering;

use crate::parser::ast::BinaryOp;

use super::{RuntimeError, Value};

pub(crate) fn parse(text: &str) -> Result<Value, RuntimeError> {
    let text = text.trim();
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()) {
        return text
            .parse::<i64>()
            .map(Value::Integer)
            .map_err(|_| RuntimeError::new("integer is outside the signed 64-bit range"));
    }
    let number = text
        .parse::<f64>()
        .map_err(|_| RuntimeError::new(format!("cannot convert '{text}' to Number")))?;
    finite(number)?;
    if number == 0.0
        && text
            .split(['e', 'E'])
            .next()
            .unwrap_or("")
            .bytes()
            .any(|c| (b'1'..=b'9').contains(&c))
    {
        return Err(RuntimeError::new(
            "number is too small to represent without underflow",
        ));
    }
    Ok(Value::Number(number))
}

pub(crate) fn finite(number: f64) -> Result<f64, RuntimeError> {
    if number.is_finite() {
        Ok(number)
    } else {
        Err(RuntimeError::new("numeric result must be finite"))
    }
}

pub(crate) fn is_number(value: &Value) -> bool {
    matches!(value, Value::Integer(_) | Value::Number(_))
}

/// Arithmetic must not silently round a large integer merely to mix representations.
pub(crate) fn exact_float(value: &Value, operation: &str) -> Result<f64, RuntimeError> {
    match value {
        Value::Integer(integer) => {
            let number = *integer as f64;
            if number as i128 != *integer as i128 {
                Err(RuntimeError::new(format!(
                    "{operation} would lose integer precision when converting to a decimal"
                )))
            } else {
                Ok(number)
            }
        }
        Value::Number(number) => finite(*number),
        other => Err(RuntimeError::new(format!(
            "{operation} requires Number, got {}",
            other.type_name()
        ))),
    }
}

pub(crate) fn negate(value: &Value) -> Result<Value, RuntimeError> {
    match value {
        Value::Integer(integer) => integer
            .checked_neg()
            .map(Value::Integer)
            .ok_or_else(|| RuntimeError::new("integer overflow in unary '-'")),
        _ => Ok(Value::Number(-exact_float(value, "unary '-'")?)),
    }
}

pub(crate) fn binary(left: &Value, op: &BinaryOp, right: &Value) -> Result<Value, RuntimeError> {
    use BinaryOp::*;
    if let (Value::Integer(a), Value::Integer(b)) = (left, right) {
        let result = match op {
            Add => a.checked_add(*b),
            Subtract => a.checked_sub(*b),
            Multiply => a.checked_mul(*b),
            FloorDivide | Modulo => {
                if *b == 0 {
                    return Err(RuntimeError::new("division by zero"));
                }
                // i128 intermediates cover MIN / -1 and floor division adjustment.
                let a = *a as i128;
                let b = *b as i128;
                let mut quotient = a / b;
                let remainder = a % b;
                if remainder != 0 && (remainder < 0) != (b < 0) {
                    quotient -= 1;
                }
                i64::try_from(if matches!(op, Modulo) {
                    a - quotient * b
                } else {
                    quotient
                })
                .ok()
            }
            Power if *b >= 0 => {
                if *a == 0 {
                    Some(if *b == 0 { 1 } else { 0 })
                } else if *a == 1 {
                    Some(1)
                } else if *a == -1 {
                    Some(if b % 2 == 0 { 1 } else { -1 })
                } else {
                    u32::try_from(*b)
                        .ok()
                        .and_then(|power| a.checked_pow(power))
                }
            }
            Divide | Power => return floating(left, op, right),
            _ => unreachable!("arithmetic operator"),
        };
        return result
            .map(Value::Integer)
            .ok_or_else(|| RuntimeError::new("integer arithmetic overflow"));
    }
    floating(left, op, right)
}

fn floating(left: &Value, op: &BinaryOp, right: &Value) -> Result<Value, RuntimeError> {
    use BinaryOp::*;
    let a = exact_float(left, "arithmetic")?;
    let b = exact_float(right, "arithmetic")?;
    if matches!(op, Divide | FloorDivide | Modulo) && b == 0.0 {
        return Err(RuntimeError::new("division by zero"));
    }
    let result = match op {
        Add => a + b,
        Subtract => a - b,
        Multiply => a * b,
        Divide => a / b,
        FloorDivide => (a / b).floor(),
        Modulo => {
            let remainder = a % b;
            if remainder != 0.0 && (remainder < 0.0) != (b < 0.0) {
                remainder + b
            } else {
                remainder
            }
        }
        Power => a.powf(b),
        _ => unreachable!("arithmetic operator"),
    };
    if result == 0.0
        && a != 0.0
        && (matches!(op, Divide | Power) || matches!(op, Multiply) && b != 0.0)
    {
        return Err(RuntimeError::new("numeric arithmetic underflow"));
    }
    Ok(Value::Number(finite(result)?))
}

pub(crate) fn compare(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Integer(a), Value::Integer(b)) => Some(a.cmp(b)),
        (Value::Number(a), Value::Number(b)) => a.partial_cmp(b),
        (Value::Integer(a), Value::Number(b)) => integer_float(*a, *b),
        (Value::Number(a), Value::Integer(b)) => integer_float(*b, *a).map(Ordering::reverse),
        _ => None,
    }
}

fn integer_float(integer: i64, number: f64) -> Option<Ordering> {
    if number.is_nan() {
        return None;
    }
    if number >= 9_223_372_036_854_775_808.0 {
        return Some(Ordering::Less);
    }
    if number < -9_223_372_036_854_775_808.0 {
        return Some(Ordering::Greater);
    }
    let order = integer.cmp(&(number as i64));
    if order != Ordering::Equal {
        return Some(order);
    }
    0.0f64.partial_cmp(&number.fract())
}

pub(crate) fn index(value: &Value, min: usize, max: usize) -> Option<usize> {
    let index = match value {
        Value::Integer(n) => usize::try_from(*n).ok()?,
        Value::Number(n) if n.is_finite() && n.fract() == 0.0 && *n >= 0.0 => {
            usize::try_from(*n as u128).ok()?
        }
        _ => return None,
    };
    (min..=max).contains(&index).then_some(index)
}

pub(crate) fn count(count: usize) -> Value {
    Value::Integer(i64::try_from(count).expect("a resident collection fits in i64"))
}
