// Explicit integration suite: build the existing Rust CLI, never run Rust tests.
const { test, before } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const cp = require('node:child_process');
const root = path.resolve(__dirname, '..');
const artifacts = path.join(root, '.test-artifacts');
const target = path.join(artifacts, 'cargo');
const binary = path.join(target, 'debug', process.platform === 'win32' ? 'solx.exe' : 'solx');
const fixture = path.join(artifacts, "space & quote' fixtures");
before(() => {
  fs.mkdirSync(fixture, { recursive: true });
  cp.execFileSync('cargo', ['build', '--locked', '--offline', '--manifest-path', path.join(root, '../Cargo.toml'), '--bin', 'solx', '--target-dir', target], { stdio: 'inherit', windowsHide: true, timeout: 120_000 });
});
const execute = source => cp.spawnSync(binary, ['-e', source], { encoding: 'utf8', windowsHide: true, timeout: 10_000 });

test('verified CLI options and original example execute successfully', () => {
  const help = cp.spawnSync(binary, ['--help'], { encoding: 'utf8', windowsHide: true });
  assert.equal(help.status, 0);
  assert.match(help.stdout, /--execute/);
  assert.ok(!help.stdout.includes('--check'));
  assert.match(help.stdout, /check/);
  const example = cp.spawnSync(binary, ['--', path.join(root, 'examples/hello.solx')], { encoding: 'utf8', windowsHide: true });
  assert.equal(example.status, 0, example.stderr);
  assert.match(example.stdout, /Greeted 3 people\./);
});

test('all snippet defaults are valid Solx programs', () => {
  const snippets = require('../data/snippets.json');
  for (const [name, snippet] of Object.entries(snippets)) {
    const source = snippet.body.join('\n').replace(/\$\{\d+\|([^}]+)\|\}/g, (_m, choices) => choices.split(',')[0])
      .replace(/\$\{\d+:([^}]+)\}/g, '$1').replace(/\$\d+/g, '');
    const result = execute(source);
    assert.equal(result.status, 0, `${name}: ${result.stderr}`);
  }
});

test('confirmed library APIs run with their documented arities', () => {
  const result = execute(`import "core" import "math" import "string" import "list" import "object"
print(str(num(" 42 ")))
core.assert(core.type(math.pi) == "Number")
print(math.abs(-1), math.sqrt(4), math.floor(1.2), math.ceil(1.2), math.round(-1.5), math.min(1, 2), math.max(1, 2))
print(string.len("é"), string.upper("hi"), string.lower("HI"), string.trim(" x "), string.slice("abc", 1, 2), string.find("abc", "b"))
print(string.join(string.split("a,b", ","), "-"), string.starts_with("abc", "a"), string.ends_with("abc", "c"))
var items = [2, 1] list.append(items, 3) list.sort(items) print(list.len(items), list.find(items, 2), list.remove(items, 1))
var obj = { key = 1 } print(object.keys(obj), object.has(obj, "key"), object.remove(obj, "key"))`);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /^42/);
  const error = execute('import "core" core.error("expected failure")');
  assert.equal(error.status, 1);
  assert.match(error.stderr, /expected failure/);
});

test('real lexer, parser, and runtime errors are reported without fabricated diagnostics', () => {
  for (const [source, message] of [['var ok = 1\n@', /unexpected character '@'.*(?:at |:)2:1/], ['var ok = 1\nvar broken = )', /expected expression.*(?:at |:)2:14/], ['num("bad")', /cannot convert 'bad' to Number/]]) {
    const result = execute(source);
    assert.equal(result.status, 1);
    assert.match(result.stderr, message);
  }
});

test('runner executes files with spaces, apostrophes, and shell metacharacters', async () => {
  const { startProcess } = require('../src/runner.cjs');
  const file = path.join(fixture, '-hello & spaces.solx');
  fs.writeFileSync(file, 'print("path works")');
  let output = '';
  const result = await startProcess({ executable: binary, args: ['--', file], cwd: fixture }, text => { output += text; }).completion;
  assert.equal(result.code, 0, output);
  assert.match(output, /path works/);
});

test('checker reads unsaved buffers and maps real Unicode lexer/parser ranges without execution', async () => {
  const { startCheck } = require('../src/checker.cjs');
  const launch = { executable: binary, args: ['check', '--json', '--stdin'], cwd: fixture };
  for (const source of ["while true do end", "print('must not run') import './missing.solx'"]) {
    assert.deepEqual(await startCheck(launch, source).completion, { diagnostics: [] });
  }
  const file = path.join(fixture, 'saved.solx');
  fs.writeFileSync(file, 'var x=1');
  for (const [source, bad, code] of [["var text='😀é'\r\nvar x=)", ')', 'parse-error'], ["var text='😀é'\r\n@", '@', 'lex-error']]) {
    const { diagnostics } = await startCheck(launch, source).completion;
    assert.equal(diagnostics.length, 1);
    assert.equal(diagnostics[0].code, code);
    assert.equal(source.slice(diagnostics[0].start, diagnostics[0].end), bad);
    assert.equal(fs.readFileSync(file, 'utf8'), 'var x=1');
  }
});
