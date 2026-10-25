// API descriptions checked against docs/SPEC.md section 14 and src/stdlib/.
const entry = (signature, description, kind = 'Function') => ({ signature, description, kind });
const builtins = {
  print: entry('print(...)', 'Prints tab-separated values and a newline. Zero arguments print a blank line. Returns nil.'),
  str: entry('str(value)', 'Converts one value to its Solx text representation.'),
  num: entry('num(value)', 'Accepts one Number or a trimmed numeric String. Integer forms preserve signed 64-bit values; decimal/exponent forms use finite binary64. Invalid strings, overflow, nonfinite values and other types raise an error.'),
};
const modules = {
  fs: {
    read_text: entry('fs.read_text(path)', 'Reads a regular UTF-8 file, at most 1 MiB, relative to --fs-root (default current directory). Requires --allow-read. Root escapes and invalid encoding raise errors. Available in current source after v0.2.0.'),
    write_text: entry('fs.write_text(path, text)', 'Creates or truncates a regular file under --fs-root and writes a String of at most 1 MiB. Requires --allow-write. Returns nil; I/O failure or cancellation can leave partial effects.'),
    append_text: entry('fs.append_text(path, text)', 'Appends a String of at most 1 MiB to a regular file under --fs-root, creating it if absent. Requires --allow-write. Returns nil; repeated appends can exceed 1 MiB.'),
    exists: entry('fs.exists(path)', 'Tests for a target under --fs-root, following symlinks within the root. Requires --allow-read. Missing targets return false; other I/O errors propagate.'),
    is_file: entry('fs.is_file(path)', 'Tests whether a target under --fs-root is a regular file. Requires --allow-read. Missing targets return false.'),
    is_dir: entry('fs.is_dir(path)', 'Tests whether a target under --fs-root is a directory. Requires --allow-read. Missing targets return false.'),
    read_dir: entry('fs.read_dir(path)', 'Returns sorted immediate entry names under --fs-root. Requires --allow-read. Limited to 4096 entries and 1 MiB of UTF-8 filename bytes; invalid filename encoding raises an error.'),
    create_dir: entry('fs.create_dir(path)', 'Creates one directory under --fs-root. Requires --allow-write and an existing parent. Existing targets raise an error. Returns nil.'),
    remove_file: entry('fs.remove_file(path)', 'Removes one file or file symlink under --fs-root. Requires --allow-write. Missing targets raise an error. Returns nil.'),
    remove_dir: entry('fs.remove_dir(path)', 'Removes one empty directory under --fs-root. Requires --allow-write. Missing or nonempty directories raise an error. Returns nil.'),
    rename: entry('fs.rename(from, to)', 'Renames within --fs-root. Requires --allow-write. Existing-target replacement follows host OS rules. Returns nil.'),
  },
  path: {
    join: entry('path.join(base, child)', 'Joins two Strings using native path rules; an absolute child replaces the base. No file access or confinement check. Inputs and output are limited to 4096 UTF-8 bytes. Available in current source after v0.2.0.'),
    basename: entry('path.basename(value)', 'Returns the final normal path component, or nil for paths such as roots and parent components. Uses native rules without file access.'),
    dirname: entry('path.dirname(value)', 'Returns the parent path String, or nil when absent. A bare filename has the empty String as its parent. No file access.'),
    extension: entry('path.extension(value)', 'Returns the final extension without a dot, or nil. A leading-dot filename has no extension; a trailing dot has an empty extension. No file access.'),
    stem: entry('path.stem(value)', 'Returns the basename with its final extension removed, or nil if there is no basename. No file access.'),
    is_absolute: entry('path.is_absolute(value)', 'Tests whether a path is absolute using host OS rules. No file access; a Windows drive-relative path is not absolute.'),
  },
  time: {
    unix_ms: entry('time.unix_ms()', 'Returns exact integer milliseconds since the Unix epoch. Requires --allow-time. Wall time may move backwards when the OS adjusts it. Available in current source after v0.2.0.'),
    elapsed_ms: entry('time.elapsed_ms()', 'Returns nondecreasing exact integer milliseconds since this interpreter enabled its clock grant. Requires --allow-time; persists across REPL runs. Use differences to measure work.'),
  },
  json: {
    parse: entry('json.parse(text)', 'Parses one JSON String. Null becomes nil; integer tokens retain signed 64-bit precision. Rejects invalid JSON and unsafe numbers. Limited to 1 MiB UTF-8 and 64 container levels.'),
    stringify: entry('json.stringify(value)', 'Returns compact JSON with sorted object keys. Preserves nil as null. Rejects cycles, functions/modules, nonfinite numbers and unavailable borrows. Limited to 1 MiB UTF-8 and 64 container levels.'),
  },
  core: {
    type: entry('core.type(value)', 'Returns the annotation-style type name, including Nil, Function, and Module.'),
    assert: entry('core.assert(condition[, message])', 'Returns the condition if truthy; otherwise raises an assertion error. Optional message must be a String, even on success. Only false and nil are falsey.'),
    error: entry('core.error(message)', 'Raises a runtime error with the supplied String message.'),
  },
  math: {
    abs: entry('math.abs(x)', 'Absolute value of one finite Number.'),
    sqrt: entry('math.sqrt(x)', 'Square root of one finite, nonnegative Number. Negative inputs raise an error.'),
    floor: entry('math.floor(x)', 'Rounds one finite Number down to an integer.'),
    ceil: entry('math.ceil(x)', 'Rounds one finite Number up to an integer.'),
    round: entry('math.round(x)', 'Rounds one finite Number to the nearest integer, with halves away from zero.'),
    min: entry('math.min(...)', 'Minimum of one or more finite Numbers.'),
    max: entry('math.max(...)', 'Maximum of one or more finite Numbers.'),
    pi: entry('math.pi', 'The Number constant pi.', 'Constant'),
  },
  string: {
    replace: entry('string.replace(text, needle, replacement)', 'Replaces every nonoverlapping literal match. The needle must not be empty; the UTF-8 result is limited to 1 MiB.'),
    byte_len: entry('string.byte_len(text)', 'Returns the UTF-8 byte length as an exact Number; string.len counts Unicode scalars instead.'),
    codepoints: entry('string.codepoints(text)', 'Returns a List of exact Unicode scalar Numbers, limited to 1,048,576 entries.'),
    from_codepoints: entry('string.from_codepoints(items)', 'Builds a String from valid integral Unicode scalar Numbers. Rejects surrogates and invalid values; UTF-8 output is limited to 1 MiB.'),
    len: entry('string.len(text)', 'Counts Unicode scalar values in a String, matching #text; not bytes or grapheme clusters.'),
    upper: entry('string.upper(text)', 'Unicode uppercase conversion of a String. The result may have a different length.'),
    lower: entry('string.lower(text)', 'Unicode lowercase conversion of a String. The result may have a different length.'),
    trim: entry('string.trim(text)', 'Removes leading and trailing Unicode whitespace from a String.'),
    slice: entry('string.slice(text, start, count)', 'Returns count Unicode scalars starting at the 1-based start. Both indexes must be finite integers; count must be nonnegative and the range must fit. start = len + 1 is valid only for count = 0.'),
    find: entry('string.find(text, needle)', 'First 1-based Unicode scalar position of a literal, case-sensitive match, or nil. An empty needle returns 1.'),
    split: entry('string.split(text, separator)', 'Splits on a nonempty literal String separator. Preserves empty fields, including trailing ones.'),
    join: entry('string.join(items, separator)', 'Joins a List of Strings with a String separator. No coercion; an empty list returns an empty String.'),
    starts_with: entry('string.starts_with(text, prefix)', 'Case-sensitive String prefix test. An empty prefix matches.'),
    ends_with: entry('string.ends_with(text, suffix)', 'Case-sensitive String suffix test. An empty suffix matches.'),
  },
  list: {
    map: entry('list.map(items, callback)', 'Returns a new List of callback(value) results from a shallow snapshot. Callback effects remain; nil results are retained.'),
    filter: entry('list.filter(items, predicate)', 'Returns snapshot values for which predicate(value) is truthy. Only false and nil are falsey.'),
    fold: entry('list.fold(items, initial, callback)', 'Visits a shallow snapshot left to right using callback(accumulator, value). Empty lists return initial.'),
    sorted: entry('list.sorted(items, comparator)', 'Returns a stably sorted shallow copy. comparator(left, right) must return a finite Number: negative, zero, or positive. Callback errors propagate.'),
    len: entry('list.len(items)', 'Returns the number of List entries, matching #items.'),
    append: entry('list.append(items, value)', 'Appends to the shared List, including nil entries. Returns nil.'),
    remove: entry('list.remove(items, index)', 'Removes and returns the entry at a finite integer index in 1..len, shifting later entries left. Invalid indexes raise an error.'),
    find: entry('list.find(items, value)', 'First 1-based index using Solx structural equality, or nil if absent.'),
    sort: entry('list.sort(items)', 'Stable ascending sort in place of uniformly finite Numbers or uniformly Strings. Empty lists are accepted; invalid contents error before mutation. Returns nil; no comparator argument.'),
  },
  object: {
    keys: entry('object.keys(object)', 'Returns a new List of property names sorted lexicographically.'),
    has: entry('object.has(object, key)', 'Tests for a String property name. A property holding nil still exists. Requires Object, not Module.'),
    remove: entry('object.remove(object, key)', 'Removes and returns a String-keyed property, or nil when absent, without a warning. Requires Object, not Module.'),
  },
};
const keywords = {
  var: 'Declare a local variable.', global: 'Declare a file-wide variable.', const: 'Declare a binding that cannot be reassigned.',
  fn: 'Declare a named or anonymous function.', if: 'Start a conditional.', then: 'Start an if or elseif body.',
  elseif: 'Add a conditional branch.', else: 'Add a fallback branch.', end: 'Close a function, conditional, loop, or try/catch.',
  try: 'Run a block with a runtime error handler.', catch: 'Handle a runtime error as an Object with message, location, and trace fields.',
  for: 'Start a numeric or collection loop.', in: 'Iterate a snapshot of List values or sorted Object keys, optionally with an index/key and value.', while: 'Repeat while a condition is truthy.', do: 'Start a loop body.',
  break: 'Exit the nearest loop in this function.', continue: 'Continue the nearest loop in this function.',
  return: 'Return one value, or nil when bare, from a function.', import: 'Import an explicit built-in module or .solx file.',
  and: 'Logical conjunction.', or: 'Logical disjunction.', not: 'Logical negation.',
  true: 'Boolean true.', false: 'Boolean false.', nil: 'A storable absence value. Assignment retains the binding; collection entries require explicit removal.',
};
const types = ['Number', 'String', 'Boolean', 'List', 'Object', 'Module', 'Function', 'Nil'];
module.exports = { builtins, modules, keywords, types };
