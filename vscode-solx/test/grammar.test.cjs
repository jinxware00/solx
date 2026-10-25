const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { before, after, test } = require('node:test');
const { Registry, INITIAL, parseRawGrammar } = require('vscode-textmate');
const { loadWASM, OnigScanner, OnigString } = require('vscode-oniguruma');

const root = path.resolve(__dirname, '..');
const readJSON = (file) => JSON.parse(fs.readFileSync(path.join(root, file), 'utf8'));
let registry;
let grammar;

before(async () => {
  await loadWASM(fs.readFileSync(require.resolve('vscode-oniguruma/release/onig.wasm')));
  registry = new Registry({
    onigLib: Promise.resolve({
      createOnigScanner: (patterns) => new OnigScanner(patterns),
      createOnigString: (text) => new OnigString(text),
    }),
    loadGrammar: async (scope) => {
      assert.equal(scope, 'source.solx');
      const file = path.join(root, 'syntaxes/solx.tmLanguage.json');
      return parseRawGrammar(fs.readFileSync(file, 'utf8'), file);
    },
  });
  grammar = await registry.loadGrammar('source.solx');
  assert.ok(grammar);
});

after(() => registry?.dispose());

function tokenize(source) {
  let stack = INITIAL;
  return source.split(/\r?\n/).map((line) => {
    const result = grammar.tokenizeLine(line, stack);
    assert.equal(result.stoppedEarly, false);
    stack = result.ruleStack;
    return {
      line,
      tokens: result.tokens.map((token) => ({
        ...token,
        text: line.slice(token.startIndex, token.endIndex),
      })),
    };
  });
}

function expectScope(line, text, scope, occurrence = 0) {
  let start = -1;
  for (let i = 0; i <= occurrence; i++) start = line.line.indexOf(text, start + 1);
  assert.notEqual(start, -1, `Missing ${JSON.stringify(text)} in ${line.line}`);
  for (let i = start; i < start + text.length; i++) {
    const token = line.tokens.find((token) => token.startIndex <= i && i < token.endIndex);
    assert.ok(token?.scopes.includes(scope),
      `${JSON.stringify(text)} at ${i} in ${line.line}: expected ${scope}, got ${token?.scopes}`);
  }
}

test('current runtime leaves braces literal in ordinary and raw strings', () => {
  const lines = tokenize('var plain = "Hi {name}, {name + 1}"\nvar raw = """Hi {name}\n{print(42)}"""');
  for (const line of lines) {
    assert.ok(line.tokens.every(token => !token.scopes.some(scope => scope.includes('interpolation'))),
      'String braces are literal at runtime: ' + line.line);
  }
});

test('raw strings carry multiline scopes and do not process escapes or comments', () => {
  const lines = tokenize('var text = """first {name}\n-- print(42) \\q "one" ""two""\nlast""" print(1)');
  expectScope(lines[0], 'first {name}', 'string.quoted.triple.double.solx');
  expectScope(lines[1], '-- print(42) \\q "one" ""two""', 'string.quoted.triple.double.solx');
  expectScope(lines[2], 'last', 'string.quoted.triple.double.solx');
  expectScope(lines[2], 'print', 'support.function.builtin.solx');
  assert.ok(lines[1].tokens.every(token => !token.scopes.some(scope =>
    scope.startsWith('comment.') || scope.startsWith('invalid.') || scope.includes('escape'))));
});

test('raw closing delimiters ignore preceding backslashes and empty literals recover', () => {
  const lines = tokenize('var text = """raw\\""" print(1)\nvar empty = """""" print(2)\nvar open = """unterminated\nprint(3)');
  expectScope(lines[0], 'raw\\', 'string.quoted.triple.double.solx');
  expectScope(lines[0], 'print', 'support.function.builtin.solx');
  expectScope(lines[1], 'print', 'support.function.builtin.solx');
  expectScope(lines[3], 'print(3)', 'string.quoted.triple.double.solx');
});

test('raw imports preserve strings and contextual aliases across a multiline literal', () => {
  const lines = tokenize('import """math""" as maths\nimport """./folder/\nmodule.solx""" as localModule -- after');
  expectScope(lines[0], 'import', 'keyword.control.import.solx');
  expectScope(lines[0], 'math', 'string.quoted.triple.double.solx');
  expectScope(lines[0], 'as', 'keyword.control.import.solx');
  expectScope(lines[0], 'maths', 'entity.name.namespace.solx');
  expectScope(lines[1], './folder/', 'string.quoted.triple.double.solx');
  expectScope(lines[2], 'module.solx', 'string.quoted.triple.double.solx');
  expectScope(lines[2], 'localModule', 'entity.name.namespace.solx');
  expectScope(lines[2], '-- after', 'comment.line.double-dash.solx');
});

test('manifest contributes the Solx grammar and editing resources', () => {
  const manifest = readJSON('package.json');
  const language = manifest.contributes.languages.find((item) => item.id === 'solx');
  const contribution = manifest.contributes.grammars.find((item) => item.language === 'solx');
  assert.ok(language.extensions.includes('.solx'));
  assert.ok(language.aliases.includes('Solx'));
  assert.equal(contribution.scopeName, 'source.solx');
  assert.equal(readJSON(contribution.path).scopeName, contribution.scopeName);
  const config = readJSON(language.configuration);
  assert.equal(config.comments.lineComment, '--');
  assert.deepEqual(config.brackets, [['{', '}'], ['[', ']'], ['(', ')']]);
  for (const pair of config.autoClosingPairs) {
    assert.deepEqual(pair.notIn, ['string', 'comment']);
  }
  const words = new RegExp(config.wordPattern, 'g');
  assert.deepEqual('score_2 math.sqrt _value'.match(words), ['score_2', 'math', 'sqrt', '_value']);
});

test('declarations, annotations, and functions use distinct scopes', () => {
  const lines = tokenize('const score: Number = 12\nglobal ready: Boolean = true\nfn add(a, b)\n    return a + b\nend\nvar double = fn(x) return x * 2 end');
  expectScope(lines[0], 'const', 'storage.modifier.solx');
  expectScope(lines[0], 'score', 'variable.other.readwrite.solx');
  expectScope(lines[0], 'Number', 'storage.type.solx');
  expectScope(lines[1], 'global', 'storage.modifier.solx');
  expectScope(lines[1], 'true', 'constant.language.solx');
  expectScope(lines[2], 'fn', 'storage.type.function.solx');
  expectScope(lines[2], 'add', 'entity.name.function.solx');
  expectScope(lines[3], 'return', 'keyword.control.solx');
  expectScope(lines[4], 'end', 'keyword.control.solx');
  expectScope(lines[5], 'fn', 'storage.type.function.solx');
  for (const type of ['Number', 'String', 'Boolean', 'List', 'Object', 'Module', 'Function', 'Nil']) {
    expectScope(tokenize(`var value: ${type} = nil`)[0], type, 'storage.type.solx');
    expectScope(tokenize(`var ${type} = nil`)[0], type, 'variable.other.readwrite.solx');
  }
});

test('all implemented control words, logical operators, and reserved error handling', () => {
  for (const word of ['break', 'continue', 'do', 'else', 'elseif', 'end', 'for', 'in', 'if', 'import', 'return', 'then', 'while']) {
    expectScope(tokenize(word)[0], word, 'keyword.control.solx');
  }
  for (const word of ['and', 'or', 'not']) {
    expectScope(tokenize(word)[0], word, 'keyword.operator.logical.solx');
  }
  for (const word of ['true', 'false', 'nil']) {
    expectScope(tokenize(word)[0], word, 'constant.language.solx');
  }
  for (const word of ['try', 'catch']) {
    expectScope(tokenize(word)[0], word, 'keyword.control.reserved.solx');
  }
  for (const name of ['as', 'pcall', 'function', 'local', 'variety', 'endless', 'Numbered']) {
    expectScope(tokenize(`var ${name} = nil`)[0], name, 'variable.other.readwrite.solx');
  }
});

test('import aliases are contextual and strings remain strings', () => {
  for (const quote of ['"', "'"]) {
    const line = tokenize(`import ${quote}./math.solx${quote} as maths -- as ignored`)[0];
    expectScope(line, 'import', 'keyword.control.import.solx');
    expectScope(line, './math.solx', `string.quoted.${quote === '"' ? 'double' : 'single'}.solx`);
    expectScope(line, 'as', 'keyword.control.import.solx');
    expectScope(line, 'maths', 'entity.name.namespace.solx');
    expectScope(line, '-- as ignored', 'comment.line.double-dash.solx');
  }
  expectScope(tokenize('var as = "as"')[0], 'as', 'variable.other.readwrite.solx');
  expectScope(tokenize('import "math"')[0], 'math', 'string.quoted.double.solx');
});

test('comments and quoted strings isolate keywords and recover at a newline', () => {
  const lines = tokenize('-- fn fake() "quoted" 123\nvar text = "-- fn fake() 123"\nvar single = \'-- import "math"\'\nvar broken = "unterminated\nvar next = 2');
  expectScope(lines[0], 'fn fake() "quoted" 123', 'comment.line.double-dash.solx');
  expectScope(lines[1], '-- fn fake() 123', 'string.quoted.double.solx');
  expectScope(lines[2], '-- import "math"', 'string.quoted.single.solx');
  expectScope(lines[3], 'unterminated', 'string.quoted.double.solx');
  expectScope(lines[4], 'var', 'storage.modifier.solx');
  expectScope(lines[4], '2', 'constant.numeric.solx');
});

test('supported escapes and invalid escapes receive separate scopes', () => {
  for (const escaped of ['n', 'r', 't', '\\', '"', "'", '{', '}']) {
    const line = tokenize(`var text = "\\${escaped}"`)[0];
    expectScope(line, `\\${escaped}`, 'constant.character.escape.solx');
  }
  const line = tokenize(String.raw`var text = "bad\q good\" -- still string" -- comment`)[0];
  expectScope(line, '\\q', 'invalid.illegal.escape.solx');
  expectScope(line, '-- still string', 'string.quoted.double.solx');
  expectScope(line, '-- comment', 'comment.line.double-dash.solx');
});

test('ordinary string braces remain literal while explicit escapes retain their scopes', () => {
  const lines = tokenize(String.raw`var text = "Hi {name}, \{literal}, {name + 1}"
var single = '{name}'`);
  expectScope(lines[0], 'name', 'string.quoted.double.solx');
  expectScope(lines[0], '\\{', 'constant.character.escape.solx');
  expectScope(lines[0], 'literal', 'string.quoted.double.solx');
  expectScope(lines[0], 'name + 1', 'string.quoted.double.solx');
  expectScope(lines[1], '{name}', 'string.quoted.single.solx');
  assert.equal(lines[0].tokens.filter((token) => token.scopes.includes('variable.other.interpolation.solx')).length, 0);
  assert.ok(lines[1].tokens.every((token) => !token.scopes.includes('meta.interpolation.solx')));
});

test('built-in calls, standard APIs, aliases, and ordinary members', () => {
  for (const name of ['print', 'str', 'num']) {
    expectScope(tokenize(`${name}(value)`)[0], name, 'support.function.builtin.solx');
    expectScope(tokenize(`player.${name}(value)`)[0], name, 'entity.name.function.call.solx');
  }
  const modules = {
    core: ['type', 'assert', 'error'],
    math: ['abs', 'sqrt', 'floor', 'ceil', 'round', 'min', 'max'],
    string: ['len', 'upper', 'lower', 'trim', 'slice', 'find', 'split', 'join', 'starts_with', 'ends_with'],
    list: ['len', 'append', 'remove', 'find', 'sort'],
    object: ['keys', 'has', 'remove'],
  };
  for (const [module, names] of Object.entries(modules)) {
    for (const name of names) {
      const line = tokenize(`${module} . ${name}(value)`)[0];
      expectScope(line, module, 'support.module.solx');
      expectScope(line, name, 'support.function.solx');
    }
  }
  expectScope(tokenize('math.pi')[0], 'pi', 'constant.language.math.solx');
  expectScope(tokenize('math.sqrt_extra(value)')[0], 'sqrt_extra', 'entity.name.function.call.solx');
  expectScope(tokenize('maths.sqrt(value)')[0], 'sqrt', 'entity.name.function.call.solx');
  expectScope(tokenize('player.score')[0], 'score', 'variable.other.property.solx');
  expectScope(tokenize('fn print(value)')[0], 'print', 'entity.name.function.solx');
});

test('numbers, concatenation, decimal points, and longest operators', () => {
  for (const number of ['0', '12', '3.5', '.25', '1e3', '2.5e-2', '4E+10']) {
    expectScope(tokenize(`var n = ${number}`)[0], number, 'constant.numeric.solx');
  }
  const line = tokenize('print(1..2, .5..1e3, value..other)')[0];
  expectScope(line, '1', 'constant.numeric.solx');
  expectScope(line, '2', 'constant.numeric.solx');
  expectScope(line, '.5', 'constant.numeric.solx');
  expectScope(line, '1e3', 'constant.numeric.solx');
  for (let i = 0; i < 3; i++) expectScope(line, '..', 'keyword.operator.solx', i);
  expectScope(line, 'other', 'variable.other.readwrite.solx');
  for (const operator of ['+', '-', '*', '/', '//', '%', '^', '#', '=', '==', '~=', '<', '>', '<=', '>=', '..']) {
    expectScope(tokenize(`a ${operator} b`)[0], operator, 'keyword.operator.solx');
  }
  expectScope(tokenize('var score123 = 4')[0], 'score123', 'variable.other.readwrite.solx');
});

test('the bundled example tokenizes with no invalid tokens or scope leaks', () => {
  const source = fs.readFileSync(path.join(root, 'examples/hello.solx'), 'utf8');
  const lines = tokenize(source);
  assert.ok(lines.length > 20);
  for (const { line, tokens } of lines) {
    assert.ok(tokens.every((token) => !token.scopes.some((scope) => scope.startsWith('invalid.'))), line);
  }
  const final = lines.find((line) => line.line === 'print("Done!")');
  assert.ok(final);
  expectScope(final, 'print', 'support.function.builtin.solx');
  expectScope(final, 'Done!', 'string.quoted.double.solx');
});
