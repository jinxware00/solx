# Files, paths, and clocks

These APIs are available in the current source checkout. They are **not included
in the published v0.2.0 download**. Build current source or use a matching CI
artifact. The standard library now has nine modules and 57 functions, plus the
three global functions `print`, `str`, and `num` and the constant `math.pi`.

## Running a program

```sh
solx --allow-read --allow-write --allow-time --fs-root . examples/file_io.solx
```

This example writes `solx-example.json` in the selected directory. Choose a
directory for your program's data with `--fs-root DIR`. It must already exist.
Omitting the root uses the process's current directory. The source file's
directory does not change that default. `--fs-root` alone grants no access;
`--allow-read`, `--allow-write`, and `--allow-time` are independent grants.
Importing a module does not grant access or perform I/O.

The VS Code extension supplies highlighting, completions and hover for these
APIs. Run programs that need grants from its terminal using the flags above.
The extension's Run command currently uses the interpreter's default grants.
Syntax checking never executes file or clock functions and requires no grants.

## `fs`: bounded file I/O

Every path is a String relative to the root. `.` names the root. Empty paths,
absolute paths, parent (`..`) components, colons, NUL characters, and paths over
4096 UTF-8 bytes are rejected. Separators follow the host operating system;
`/` works on all supported systems. Paths are resolved through a `cap-std`
directory handle. Symlink resolution cannot leave the granted directory;
relative symlinks within it are supported. A symlink to an absolute target is
rejected even when that target is inside the root. Removing or renaming a
symlink acts on the link itself.

| Function | Grant | Result |
| --- | --- | --- |
| `fs.read_text(path)` | Read | Read a regular file as a UTF-8 String, at most 1 MiB. Preserves NUL, BOM and line-ending bytes; invalid UTF-8 errors. |
| `fs.write_text(path, text)` | Write | Create or truncate a regular file and write the String; return nil. Validate types, paths and the 1 MiB payload limit before creating/truncating. |
| `fs.append_text(path, text)` | Write | Append the String to a regular file, creating it if absent; return nil. At most 1 MiB per call. |
| `fs.exists(path)` | Read | Boolean; follows symlinks. False only for a missing target; permission and other I/O errors propagate. |
| `fs.is_file(path)` | Read | Whether the target is a regular file; false for missing targets. |
| `fs.is_dir(path)` | Read | Whether the target is a directory; false for missing targets. |
| `fs.read_dir(path)` | Read | List of immediate entry names, sorted lexicographically, without `.` or `..`. At most 4096 entries and 1 MiB of UTF-8 filename bytes. Invalid filename encoding errors. |
| `fs.create_dir(path)` | Write | Create one directory; parent must exist and target must be absent. Return nil. |
| `fs.remove_file(path)` | Write | Remove one file or file symlink; return nil. Missing paths error. |
| `fs.remove_dir(path)` | Write | Remove an empty directory; return nil. Missing or nonempty directories error. No recursive removal. |
| `fs.rename(from, to)` | Write | Rename within the granted root; return nil. Parent directories must exist. Replacement and cross-filesystem behavior follow the host OS. |

Reads return a complete String or an error; directory listings return a complete
List or an error. Handles close on success, error, or cancellation. Writes are
not transactions: a failed or cancelled operation may leave a created, truncated
or partially written file. Successful return does not guarantee durable storage
after a power loss. Append writes use append mode; concurrent writers may
interleave chunks. Repeated appends may grow a file beyond 1 MiB. The per-call
limits are not a disk quota or a general memory budget.

Permission, I/O, encoding and per-call bound failures are ordinary catchable
runtime errors. Directory traversal checks the run budget for every entry;
text I/O checks it between chunks of at most 8 KiB. Step exhaustion, deadlines
and cancellation remain noncatchable. Checks are cooperative: a blocking OS
operation cannot be interrupted by the interpreter's deadline. Nonregular
files are rejected, and Unix opens use nonblocking mode to avoid FIFO waits.

These grants govern only the new `fs` API. Entry-file reads and existing `.solx`
imports keep their existing behavior. They do not provide a whole-interpreter
sandbox. A host should select a trusted root: existing hard links and special
mounts can expose data through names already inside that directory.

## `path`: pure path operations

No grant is required. Functions use native platform path rules without reading
files, expanding `~`/environment variables, resolving symlinks, or canonicalizing
paths. Inputs and outputs are limited to 4096 UTF-8 bytes; NUL is invalid.

| Function | Result |
| --- | --- |
| `path.join(base, child)` | Joined String. An absolute child replaces the base; Windows rooted/drive paths follow Windows rules. `..` is preserved. This is not a confinement check. |
| `path.basename(value)` | Final normal component or nil (for example a root or `..`). Trailing separators are ignored. |
| `path.dirname(value)` | Parent String, or nil if there is no parent. A bare filename has the empty String as parent. |
| `path.extension(value)` | Final suffix without the dot, or nil. `.hidden` has no extension; `file.` has an empty extension. |
| `path.stem(value)` | Basename with the final extension removed, or nil if there is no basename. |
| `path.is_absolute(value)` | Boolean using host rules. A drive-relative Windows path is not absolute. |

## `time`: clocks

Both functions require `--allow-time` and return exact integer Numbers, measured
in milliseconds. `time.unix_ms()` returns wall time since 1970-01-01 UTC; the OS
can adjust it forwards or backwards. A clock before the epoch or outside the
signed 64-bit range errors. `time.elapsed_ms()` returns nondecreasing monotonic
time since the host enabled the grant. It persists across runs in the same REPL
or interpreter. Use differences between readings to measure a task. A new host
configuration starts a new elapsed clock. There is no sleep or global shared
clock state.

## Embedding

`Interpreter::new()` and `Interpreter::with_limits(...)` deny native I/O by
default. Supply grants explicitly:

```rust
use solx::{host::HostCapabilities, interpreter::Interpreter};

let mut interpreter = Interpreter::new();
let grants = HostCapabilities::default()
    .with_filesystem("data", true, false)? // existing directory; read only
    .with_time();
interpreter.set_capabilities(grants);
# Ok::<(), std::io::Error>(())
```

`set_capabilities` replaces previous grants and closes the old root handle.
Even cached or saved native Function values consult the current configuration
at invocation. Each interpreter owns its configuration; no current-directory
changes or process-global grants are used.
