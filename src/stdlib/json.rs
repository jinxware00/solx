//! Bounded JSON conversion. Container recursion is checked before descending;
//! strings and numbers use serde_json's JSON syntax and escaping.

use std::collections::HashSet;
use std::io::{self, Write};
use std::rc::Rc;

use super::{Arguments, LibraryFunction, NativeResult};
use crate::runtime::{RuntimeError, Value, read_collection};

const MAX_BYTES: usize = 1024 * 1024;
const MAX_DEPTH: usize = 64;

pub(super) const FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::fixed("json.parse", 1, parse),
    LibraryFunction::fixed("json.stringify", 1, stringify),
];

fn error(operation: &str, message: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(format!("{operation}(): {message}"))
}

fn parse(args: &Arguments<'_>) -> NativeResult {
    let text = args.string(0)?;
    if text.len() > MAX_BYTES {
        return Err(error(
            args.name,
            format_args!("input exceeds {MAX_BYTES} UTF-8 bytes"),
        ));
    }
    preflight(text)?;
    // from_str also rejects trailing non-whitespace data. The preflight depth
    // bound applies before serde_json allocates or recursively parses values.
    let parsed = serde_json::from_str(text)
        .map_err(|cause| error(args.name, format_args!("invalid JSON: {cause}")))?;
    from_json(parsed)
}

/// Scan outside strings before parsing. This is a resource/numeric guard, not
/// a second JSON grammar: serde_json remains responsible for syntax validation.
fn preflight(text: &str) -> Result<(), RuntimeError> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut offset = 0;
    while offset < bytes.len() {
        let byte = bytes[offset];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
        } else {
            match byte {
                b'"' => in_string = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return Err(error(
                            "json.parse",
                            format_args!("nesting exceeds {MAX_DEPTH} containers"),
                        ));
                    }
                }
                b']' | b'}' => depth = depth.saturating_sub(1),
                b'-' | b'0'..=b'9' => {
                    let start = offset;
                    while offset < bytes.len()
                        && matches!(
                            bytes[offset],
                            b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'
                        )
                    {
                        offset += 1;
                    }
                    validate_number_token(&text[start..offset])?;
                    continue;
                }
                _ => {}
            }
        }
        offset += 1;
    }
    Ok(())
}

fn validate_number_token(token: &str) -> Result<(), RuntimeError> {
    // Invalid tokens are left for serde_json's source-positioned diagnostics.
    let Ok(number) = token.parse::<f64>() else {
        return Ok(());
    };
    if !token.contains(['.', 'e', 'E']) && token.parse::<i64>().is_err() {
        return Err(error(
            "json.parse",
            "integer is outside the signed 64-bit range",
        ));
    }
    if !number.is_finite() {
        return Err(error(
            "json.parse",
            "number is outside the finite Number range",
        ));
    }
    let mantissa = token.split(['e', 'E']).next().expect("nonempty token");
    if number == 0.0 && mantissa.bytes().any(|byte| matches!(byte, b'1'..=b'9')) {
        return Err(error("json.parse", "nonzero number underflows to zero"));
    }
    Ok(())
}

fn from_json(value: serde_json::Value) -> NativeResult {
    Ok(match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(value) => Value::Boolean(value),
        serde_json::Value::String(value) => Value::String(value),
        serde_json::Value::Number(value) => {
            if let Some(integer) = value.as_i64() {
                Value::Integer(integer)
            } else if value.is_u64() {
                return Err(error(
                    "json.parse",
                    "integer is outside the signed 64-bit range",
                ));
            } else {
                let number = value.as_f64().ok_or_else(|| {
                    error("json.parse", "number is outside the finite Number range")
                })?;
                if !number.is_finite() {
                    return Err(error(
                        "json.parse",
                        "number is outside the finite Number range",
                    ));
                }
                Value::Number(number)
            }
        }
        serde_json::Value::Array(items) => {
            Value::list(items.into_iter().map(from_json).collect::<Result<_, _>>()?)
        }
        serde_json::Value::Object(properties) => Value::object(
            properties
                .into_iter()
                .map(|(key, value)| Ok((key, from_json(value)?)))
                .collect::<Result<_, RuntimeError>>()?,
        ),
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Identity {
    List(usize),
    Object(usize),
}

#[derive(Default)]
struct Output {
    bytes: Vec<u8>,
    active: HashSet<Identity>,
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BYTES - self.bytes.len() {
            return Err(io::Error::other(format!(
                "output exceeds {MAX_BYTES} UTF-8 bytes"
            )));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Output {
    fn raw(&mut self, bytes: &[u8]) -> Result<(), RuntimeError> {
        self.write_all(bytes)
            .map_err(|cause| error("json.stringify", cause))
    }

    fn enter(&mut self, identity: Identity, depth: usize) -> Result<(), RuntimeError> {
        if depth == MAX_DEPTH {
            return Err(error(
                "json.stringify",
                format_args!("nesting exceeds {MAX_DEPTH} containers"),
            ));
        }
        if !self.active.insert(identity) {
            return Err(error(
                "json.stringify",
                "cannot serialize cyclic collections",
            ));
        }
        Ok(())
    }

    fn value(&mut self, value: &Value, depth: usize) -> Result<(), RuntimeError> {
        match value {
            Value::Nil => self.raw(b"null"),
            Value::Boolean(true) => self.raw(b"true"),
            Value::Boolean(false) => self.raw(b"false"),
            Value::Integer(number) => {
                serde_json::to_writer(self, number).map_err(|cause| error("json.stringify", cause))
            }
            Value::Number(number) => {
                if !number.is_finite() {
                    return Err(error(
                        "json.stringify",
                        "cannot serialize a nonfinite Number",
                    ));
                }
                serde_json::to_writer(self, number).map_err(|cause| error("json.stringify", cause))
            }
            Value::String(text) => {
                serde_json::to_writer(self, text).map_err(|cause| error("json.stringify", cause))
            }
            Value::List(items) => {
                let identity = Identity::List(Rc::as_ptr(items) as usize);
                self.enter(identity, depth)?;
                let items = read_collection(items, "json.stringify()")?;
                // Each element needs at least one byte plus a separator. Reject
                // oversized host collections before traversing their contents.
                if items.len() > (MAX_BYTES - self.bytes.len()) / 2 {
                    return Err(error(
                        "json.stringify",
                        format_args!("output exceeds {MAX_BYTES} UTF-8 bytes"),
                    ));
                }
                self.raw(b"[")?;
                for (index, item) in items.iter().enumerate() {
                    if index != 0 {
                        self.raw(b",")?;
                    }
                    self.value(item, depth + 1)?;
                }
                self.raw(b"]")?;
                self.active.remove(&identity);
                Ok(())
            }
            Value::Object(properties) => {
                let identity = Identity::Object(Rc::as_ptr(properties) as usize);
                self.enter(identity, depth)?;
                let properties = read_collection(properties, "json.stringify()")?;
                // The shortest entry is an empty key with a one-byte value.
                // This also bounds the temporary vector needed to sort keys.
                if properties.len() > (MAX_BYTES - self.bytes.len()) / 5 {
                    return Err(error(
                        "json.stringify",
                        format_args!("output exceeds {MAX_BYTES} UTF-8 bytes"),
                    ));
                }
                let mut keys: Vec<_> = properties.keys().collect();
                keys.sort_unstable();
                self.raw(b"{")?;
                for (index, key) in keys.into_iter().enumerate() {
                    if index != 0 {
                        self.raw(b",")?;
                    }
                    serde_json::to_writer(&mut *self, key)
                        .map_err(|cause| error("json.stringify", cause))?;
                    self.raw(b":")?;
                    self.value(&properties[key], depth + 1)?;
                }
                self.raw(b"}")?;
                self.active.remove(&identity);
                Ok(())
            }
            Value::Function(_) | Value::NativeFunction(_) | Value::Module(_) => Err(error(
                "json.stringify",
                format_args!("argument 1 contains unsupported {}", value.type_name()),
            )),
        }
    }
}

fn stringify(args: &Arguments<'_>) -> NativeResult {
    let mut output = Output::default();
    output.value(&args.values[0], 0)?;
    // Only ASCII syntax and serde_json-escaped UTF-8 strings were written.
    Ok(Value::String(
        String::from_utf8(output.bytes).expect("JSON output is UTF-8"),
    ))
}
