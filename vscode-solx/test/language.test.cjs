const { test } = require('node:test');
const assert = require('node:assert/strict');
const { completions, hover, scan } = require('../src/language.cjs');
const complete = source => completions(source, source.length);
const labels = source => complete(source).map(item => item.label);
const at = (source, needle) => hover(source, source.lastIndexOf(needle));

test('confirmed keywords, built-ins, and valid construct snippets', () => {
  const items = complete('');
  for (const name of ['var', 'fn', 'if', 'for', 'while', 'import', 'print', 'str', 'num']) assert.ok(items.some(item => item.label === name));
  for (const name of ['pcall', 'try', 'catch', 'function', 'local', 'pairs', 'require', 'random']) assert.ok(!items.some(item => item.label === name));
  assert.equal(items.find(item => item.label === 'print').kind, 'Function');
  assert.match(items.find(item => item.kind === 'Snippet' && item.label === 'fn').insertText, /^fn .*\n\t\$0\nend$/);
  assert.deepEqual(labels('pri'), ['print']);
  assert.deepEqual(labels('var name: '), ['Number', 'String', 'Boolean', 'List', 'Object', 'Module', 'Function', 'Nil']);
  assert.deepEqual(labels('fn f(argument: '), []);
  assert.deepEqual(labels('import "math" '), ['as']);
  assert.deepEqual(labels('var '), []);
});

test('completions and hover are quiet inside comments and both string forms', () => {
  for (const source of ['-- print', 'var x = "print', "var x = 'print", 'var x = "escaped\\" print', '-- import "math"\n-- math.']) {
    assert.deepEqual(complete(source), [], source);
    assert.equal(at(source, 'print'), undefined, source);
  }
  assert.deepEqual(labels('var x = "unterminated\npri'), ['print']);
  assert.deepEqual(labels('-- comment\r\npri'), ['print']);
  assert.deepEqual(labels('var x = "-- comment"\npri'), ['print']);
  assert.equal(at('var text = "{print}"', 'print'), undefined);
});

test('standard modules require an explicit import and support aliases', () => {
  assert.deepEqual(labels('math.'), []);
  assert.deepEqual(labels('import "math"\nmath.sq'), ['sqrt']);
  assert.equal(complete('import "math"\nmath.p')[0].kind, 'Constant');
  assert.deepEqual(labels('import "math" as m\nm.sq'), ['sqrt']);
  assert.deepEqual(labels('import "math" as m\nmath.'), []);
  assert.deepEqual(labels('import "./math.solx"\nmath.'), []);
  assert.deepEqual(labels('-- import "math"\nmath.'), []);
  assert.deepEqual(labels('var x = "import \\"math\\""\nmath.'), []);
  assert.deepEqual(labels('import "math"\nplayer.math.'), []);
  assert.deepEqual(labels('player.'), []);
});

test('hover signatures and descriptions match implemented API behavior', () => {
  assert.equal(at('print(1)', 'print').signature, 'print(...)');
  assert.match(at('num("42")', 'num').description, /trimmed numeric String/);
  assert.match(at('import "string"\nstring.slice("abc", 1, 1)', 'slice').description, /1-based/);
  assert.match(at('import "list" as items\nitems.sort([])', 'sort').description, /no comparator/);
  assert.equal(at('import "math"\nmath.pi', 'pi').kind, 'Constant');
  for (const source of ['player.print()', 'printable()', 'pcall()', 'math.sqrt(4)', 'import "math"\nmath.sqrt_more(4)', 'import "math"\nx.math.sqrt(4)']) {
    assert.equal(hover(source, source.lastIndexOf('(') - 1), undefined, source);
  }
});

test('bindings respect ordinary lexical shadowing and invalidate assignments', () => {
  assert.deepEqual(labels('var print = fn() end\npri'), []);
  assert.equal(at('fn print(x) end\nprint(1)', 'print'), undefined);
  assert.equal(at('fn f(print)\n print(1)\nend', 'print'), undefined);
  assert.deepEqual(labels('import "math"\nfn f(math)\nmath.'), []);
  assert.deepEqual(labels('import "math"\nvar math = {}\nmath.'), []);
  assert.deepEqual(labels('import "math"\nmath = {}\nmath.'), []);
  assert.deepEqual(labels('fn f() import "math" end\nmath.'), []);
  assert.deepEqual(labels('import "math"\nif true then var math = {} end\nmath.sq'), ['sqrt']);
  assert.deepEqual(labels('fn f() import "math" as m\nm.sq'), ['sqrt']);
  assert.equal(at('fn print(x) end', 'print'), undefined);
  assert.equal(at('var obj = { print = 1 }', 'print'), undefined);
  assert.deepEqual(labels('if true then global print = 1 end\npri'), []);
});

test('UTF-16 replacement ranges, malformed input, and request size limits', () => {
  const source = '-- 😀\nimport "math"\nmath.sq';
  const item = complete(source)[0];
  assert.equal(source.slice(item.start, item.end), 'sq');
  const doc = '-- 😀\nprint(1)';
  const info = at(doc, 'print');
  assert.equal(doc.slice(info.start, info.end), 'print');
  for (const source of ['@', '"', 'fn(', 'import "', 'if (', '\\', '\0', 'end end', '1e+', 'a..print']) assert.doesNotThrow(() => complete(source));
  assert.deepEqual(complete(' '.repeat(1_000_001)), []);
  assert.ok(scan('😀').every(t => t.end > t.start));
});
