use std::path::Path;

use super::{Arguments, LibraryFunction, NativeResult};
use crate::runtime::Value;

pub(super) const MAX_PATH_BYTES: usize = 4096;

pub(super) const FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::fixed("path.join", 2, join),
    LibraryFunction::fixed("path.basename", 1, |a| {
        Ok(optional(path(a, 0)?.file_name()))
    }),
    LibraryFunction::fixed("path.dirname", 1, |a| {
        Ok(optional(path(a, 0)?.parent().map(Path::as_os_str)))
    }),
    LibraryFunction::fixed("path.extension", 1, |a| {
        Ok(optional(path(a, 0)?.extension()))
    }),
    LibraryFunction::fixed("path.stem", 1, |a| Ok(optional(path(a, 0)?.file_stem()))),
    LibraryFunction::fixed("path.is_absolute", 1, |a| {
        Ok(Value::Boolean(path(a, 0)?.is_absolute()))
    }),
];

pub(super) fn path<'a>(
    args: &Arguments<'a>,
    index: usize,
) -> Result<&'a Path, crate::runtime::RuntimeError> {
    let text = args.string(index)?;
    if text.len() > MAX_PATH_BYTES {
        return Err(args.error(index, &format!("exceeds {MAX_PATH_BYTES} UTF-8 bytes")));
    }
    if text.contains('\0') {
        return Err(args.error(index, "must not contain NUL"));
    }
    Ok(Path::new(text))
}

fn optional(text: Option<&std::ffi::OsStr>) -> Value {
    // These components came from a UTF-8 String, so no lossy conversion occurs.
    text.map_or(Value::Nil, |text| {
        Value::String(text.to_str().expect("UTF-8 path component").to_owned())
    })
}

fn join(args: &Arguments<'_>) -> NativeResult {
    let base = path(args, 0)?;
    let child = path(args, 1)?;
    let joined = base.join(child);
    let text = joined.to_str().expect("joined UTF-8 paths");
    if text.len() > MAX_PATH_BYTES {
        return Err(crate::runtime::RuntimeError::new(format!(
            "path.join() output exceeds {MAX_PATH_BYTES} UTF-8 bytes"
        )));
    }
    Ok(Value::String(text.to_owned()))
}
