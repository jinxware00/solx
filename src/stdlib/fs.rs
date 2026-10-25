use std::io::{self, Read, Write};
use std::path::{Component, Path};

use cap_std::fs::{File, OpenOptions};

use super::{Arguments, LibraryFunction, NativeResult, path};
use crate::{
    interpreter::Interpreter,
    runtime::{RuntimeError, Value},
};

const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const CHUNK_BYTES: usize = 8192;

pub(super) const FUNCTIONS: &[LibraryFunction] = &[
    LibraryFunction::with_interpreter("fs.read_text", 1, read_text),
    LibraryFunction::with_interpreter("fs.write_text", 2, |i, a| write_text(i, a, false)),
    LibraryFunction::with_interpreter("fs.append_text", 2, |i, a| write_text(i, a, true)),
    LibraryFunction::with_interpreter("fs.exists", 1, |i, a| query(i, a, |_| true)),
    LibraryFunction::with_interpreter("fs.is_file", 1, |i, a| query(i, a, |m| m.is_file())),
    LibraryFunction::with_interpreter("fs.is_dir", 1, |i, a| query(i, a, |m| m.is_dir())),
    LibraryFunction::with_interpreter("fs.read_dir", 1, read_dir),
    LibraryFunction::with_interpreter("fs.create_dir", 1, |i, a| {
        change(i, a, |dir, path| dir.create_dir(path))
    }),
    LibraryFunction::with_interpreter("fs.remove_file", 1, |i, a| {
        change(i, a, |dir, path| dir.remove_file(path))
    }),
    LibraryFunction::with_interpreter("fs.remove_dir", 1, |i, a| {
        change(i, a, |dir, path| dir.remove_dir(path))
    }),
    LibraryFunction::with_interpreter("fs.rename", 2, rename),
];

fn relative<'a>(args: &Arguments<'a>, index: usize) -> Result<&'a Path, RuntimeError> {
    let path = path::path(args, index)?;
    // Reject parent traversal even when it would remain inside the root. The
    // directory capability also enforces confinement during symlink resolution.
    // Colons exclude Windows drive paths and alternate data streams everywhere.
    if path.as_os_str().is_empty()
        || args.string(index)?.contains(':')
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(args.error(
            index,
            "must be a nonempty relative path without '..', roots, or ':'",
        ));
    }
    Ok(path)
}

fn io_error(args: &Arguments<'_>, error: io::Error) -> RuntimeError {
    RuntimeError::new(format!("{}(): {error}", args.name))
}

fn size_error(args: &Arguments<'_>) -> RuntimeError {
    RuntimeError::new(format!(
        "{}() text exceeds {MAX_TEXT_BYTES} UTF-8 bytes",
        args.name
    ))
}

fn open_regular(
    interpreter: &Interpreter,
    args: &Arguments<'_>,
    path: &Path,
    write: bool,
    append: bool,
) -> Result<File, RuntimeError> {
    let mut options = OpenOptions::new();
    if write {
        // Validate the opened handle before truncating. Appending is O_APPEND,
        // so writes use the end of the file rather than a stale seek position.
        options.write(true).create(true).append(append);
    } else {
        options.read(true);
    }
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        // Avoid waiting for a FIFO peer before checking the opened file type.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = interpreter
        .host
        .directory(write, args.name)?
        .open_with(path, &options)
        .map_err(|error| io_error(args, error))?;
    let metadata = file.metadata().map_err(|error| io_error(args, error))?;
    if !metadata.is_file() {
        return Err(RuntimeError::new(format!(
            "{}() requires a regular file",
            args.name
        )));
    }
    if !write && metadata.len() > MAX_TEXT_BYTES as u64 {
        return Err(size_error(args));
    }
    Ok(file)
}

fn read_text(interpreter: &mut Interpreter, args: &Arguments<'_>) -> NativeResult {
    let path = relative(args, 0)?;
    interpreter.native_checkpoint()?;
    let mut file = open_regular(interpreter, args, path, false, false)?;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; CHUNK_BYTES];
    loop {
        interpreter.native_checkpoint()?;
        let remaining = (MAX_TEXT_BYTES + 1 - bytes.len()).min(CHUNK_BYTES);
        let count = match file.read(&mut chunk[..remaining]) {
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_error(args, error)),
        };
        if count == 0 {
            break;
        }
        if count > MAX_TEXT_BYTES - bytes.len() {
            return Err(size_error(args));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    interpreter.native_checkpoint()?;
    String::from_utf8(bytes)
        .map(Value::String)
        .map_err(|_| RuntimeError::new("fs.read_text() file must contain valid UTF-8"))
}

fn write_text(interpreter: &mut Interpreter, args: &Arguments<'_>, append: bool) -> NativeResult {
    let path = relative(args, 0)?;
    let text = args.string(1)?;
    // All argument and payload validation precedes opening/creating/truncating.
    if text.len() > MAX_TEXT_BYTES {
        return Err(size_error(args));
    }
    interpreter.native_checkpoint()?;
    let mut file = open_regular(interpreter, args, path, true, append)?;
    if !append {
        file.set_len(0).map_err(|error| io_error(args, error))?;
    }
    for chunk in text.as_bytes().chunks(CHUNK_BYTES) {
        interpreter.native_checkpoint()?;
        file.write_all(chunk)
            .map_err(|error| io_error(args, error))?;
    }
    interpreter.native_checkpoint()?;
    Ok(Value::Nil)
}

fn query(
    interpreter: &mut Interpreter,
    args: &Arguments<'_>,
    predicate: fn(&cap_std::fs::Metadata) -> bool,
) -> NativeResult {
    let path = relative(args, 0)?;
    interpreter.native_checkpoint()?;
    let result = interpreter.host.directory(false, args.name)?.metadata(path);
    interpreter.native_checkpoint()?;
    match result {
        Ok(metadata) => Ok(Value::Boolean(predicate(&metadata))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Value::Boolean(false)),
        Err(error) => Err(io_error(args, error)),
    }
}

fn read_dir(interpreter: &mut Interpreter, args: &Arguments<'_>) -> NativeResult {
    let path = relative(args, 0)?;
    interpreter.native_checkpoint()?;
    let mut entries = interpreter
        .host
        .directory(false, args.name)?
        .read_dir(path)
        .map_err(|error| io_error(args, error))?;
    let mut names = Vec::new();
    let mut bytes = 0;
    loop {
        interpreter.native_checkpoint()?;
        let Some(entry) = entries.next() else {
            break;
        };
        let name = entry
            .map_err(|error| io_error(args, error))?
            .file_name()
            .into_string()
            .map_err(|_| RuntimeError::new("fs.read_dir() filename is not valid UTF-8"))?;
        bytes += name.len();
        if names.len() == MAX_ENTRIES || bytes > MAX_TEXT_BYTES {
            return Err(RuntimeError::new(format!(
                "fs.read_dir() exceeds {MAX_ENTRIES} entries or {MAX_TEXT_BYTES} filename bytes"
            )));
        }
        names.push(name);
    }
    names.sort_unstable();
    interpreter.native_checkpoint()?;
    Ok(Value::list(names.into_iter().map(Value::String).collect()))
}

fn change(
    interpreter: &mut Interpreter,
    args: &Arguments<'_>,
    action: impl FnOnce(&cap_std::fs::Dir, &Path) -> io::Result<()>,
) -> NativeResult {
    let path = relative(args, 0)?;
    interpreter.native_checkpoint()?;
    action(interpreter.host.directory(true, args.name)?, path)
        .map_err(|error| io_error(args, error))?;
    interpreter.native_checkpoint()?;
    Ok(Value::Nil)
}

fn rename(interpreter: &mut Interpreter, args: &Arguments<'_>) -> NativeResult {
    let from = relative(args, 0)?;
    let to = relative(args, 1)?;
    interpreter.native_checkpoint()?;
    let dir = interpreter.host.directory(true, args.name)?;
    dir.rename(from, dir, to)
        .map_err(|error| io_error(args, error))?;
    interpreter.native_checkpoint()?;
    Ok(Value::Nil)
}
