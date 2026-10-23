// API descriptions checked against docs/SPEC.md section 14 and src/stdlib/.
const entry = (signature, description, kind = 'Function') => ({ signature, description, kind });
const builtins = {
  print: entry('print(...)', 'Prints tab-separated values and a newline. Zero arguments print a blank line. Returns nil.'),
  str: entry('str(value)', 'Converts one value to its Solx text representation.'),
  num: entry('num(value)', 'Accepts one Number or a trimmed numeric String. Invalid strings and other types raise an error.'),
};
const modules = {
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
  elseif: 'Add a conditional branch.', else: 'Add a fallback branch.', end: 'Close a function, conditional, or loop.',
  for: 'Start an inclusive numeric for loop.', while: 'Repeat while a condition is truthy.', do: 'Start a loop body.',
  break: 'Exit the nearest loop in this function.', continue: 'Continue the nearest loop in this function.',
  return: 'Return one value, or nil when bare, from a function.', import: 'Import an explicit built-in module or .solx file.',
  and: 'Logical conjunction.', or: 'Logical disjunction.', not: 'Logical negation.',
  true: 'Boolean true.', false: 'Boolean false.', nil: 'Nil; assignment to an existing untyped variable deletes its binding.',
};
const types = ['Number', 'String', 'Boolean', 'List', 'Object', 'Module', 'Function', 'Nil'];
module.exports = { builtins, modules, keywords, types };
