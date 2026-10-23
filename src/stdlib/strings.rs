use super::{Arguments, LibraryFunction, NativeResult};
use crate::runtime::{RuntimeError, Value, numeric, read_collection};

const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_CODEPOINTS: usize = 1024 * 1024;

pub(super) const FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::fixed("string.len", 1, |a| {
        Ok(crate::runtime::numeric::count(a.string(0)?.chars().count()))
    }),
    LibraryFunction::fixed("string.byte_len", 1, |a| {
        Ok(numeric::count(a.string(0)?.len()))
    }),
    LibraryFunction::fixed("string.codepoints", 1, codepoints),
    LibraryFunction::fixed("string.from_codepoints", 1, from_codepoints),
    LibraryFunction::fixed("string.replace", 3, replace),
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

fn output_size_error(args: &Arguments<'_>) -> RuntimeError {
    RuntimeError::new(format!(
        "{}() output exceeds {MAX_OUTPUT_BYTES} UTF-8 bytes",
        args.name
    ))
}

fn replace(args: &Arguments<'_>) -> NativeResult {
    let text = args.string(0)?;
    let needle = args.string(1)?;
    let replacement = args.string(2)?;
    if needle.is_empty() {
        return Err(args.error(1, "must not be empty"));
    }
    // Preflight exact byte growth before allocating. Matches are literal,
    // left-to-right and nonoverlapping, and replacement text is not rescanned.
    let matches = text.matches(needle).count();
    let output_len = matches
        .checked_mul(needle.len())
        .and_then(|removed| text.len().checked_sub(removed))
        .and_then(|remaining| {
            matches
                .checked_mul(replacement.len())
                .and_then(|added| remaining.checked_add(added))
        })
        .filter(|length| *length <= MAX_OUTPUT_BYTES)
        .ok_or_else(|| output_size_error(args))?;
    let mut output = String::with_capacity(output_len);
    let mut previous = 0;
    for (offset, matched) in text.match_indices(needle) {
        output.push_str(&text[previous..offset]);
        output.push_str(replacement);
        previous = offset + matched.len();
    }
    output.push_str(&text[previous..]);
    Ok(Value::String(output))
}

fn codepoints(args: &Arguments<'_>) -> NativeResult {
    let text = args.string(0)?;
    let count = text.chars().take(MAX_CODEPOINTS + 1).count();
    if count > MAX_CODEPOINTS {
        return Err(RuntimeError::new(format!(
            "{}() output exceeds {MAX_CODEPOINTS} code points",
            args.name
        )));
    }
    let mut values = Vec::with_capacity(count);
    values.extend(
        text.chars()
            .map(|scalar| Value::Integer(i64::from(u32::from(scalar)))),
    );
    Ok(Value::list(values))
}

fn unicode_scalar(value: &Value) -> Option<char> {
    let scalar = numeric::index(value, 0, 0x10ffff)?;
    char::from_u32(scalar as u32)
}

fn from_codepoints(args: &Arguments<'_>) -> NativeResult {
    let items = read_collection(args.list(0)?, "string.from_codepoints()")?;
    // Every scalar occupies at least one UTF-8 byte, even U+0000.
    if items.len() > MAX_OUTPUT_BYTES {
        return Err(output_size_error(args));
    }
    let mut output_len = 0usize;
    for (index, value) in items.iter().enumerate() {
        if !numeric::is_number(value) {
            return Err(args.error(
                0,
                &format!(
                    "contains {} at index {}; expected Number",
                    value.type_name(),
                    index + 1
                ),
            ));
        }
        let scalar = unicode_scalar(value).ok_or_else(|| {
            args.error(
                0,
                &format!(
                    "contains an invalid Unicode scalar at index {}; expected an integer in 0..1114111 excluding 55296..57343",
                    index + 1
                ),
            )
        })?;
        output_len = output_len
            .checked_add(scalar.len_utf8())
            .filter(|length| *length <= MAX_OUTPUT_BYTES)
            .ok_or_else(|| output_size_error(args))?;
    }
    let mut output = String::with_capacity(output_len);
    for value in items.iter() {
        output.push(unicode_scalar(value).expect("validated scalars under a shared borrow"));
    }
    Ok(Value::String(output))
}

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
