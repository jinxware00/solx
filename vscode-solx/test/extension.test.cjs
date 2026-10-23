const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createRequire } = require('node:module');

function host(overrides = {}) {
  const commands = new Map();
  const messages = [];
  const launched = [];
  const doc = { languageId: 'solx', uri: { scheme: 'file', fsPath: path.resolve('example.solx') }, version: 1, isDirty: false, isClosed: false,
    save: async () => { doc.isDirty = false; return true; }, ...overrides.document };
  const output = { text: '', append(text) { this.text += text; }, appendLine(text) { this.text += text + '\n'; }, clear() { this.text = ''; }, show() {}, dispose() {} };
  let stopped = false;
  const fake = {
    window: { activeTextEditor: { document: doc }, createOutputChannel: () => output,
      showErrorMessage: text => { messages.push(text); return Promise.resolve(); },
      showWarningMessage: () => Promise.resolve(overrides.action || 'Cancel') },
    workspace: { isTrusted: overrides.trusted !== false, getWorkspaceFolder: () => undefined, getConfiguration: () => ({ get: (_key, fallback) => fallback }) },
    commands: { executeCommand: async () => {}, registerCommand: (name, fn) => { commands.set(name, fn); return { dispose() {} }; } },
    languages: { registerCompletionItemProvider: () => ({ dispose() {} }), registerHoverProvider: () => ({ dispose() {} }) },
  };
  const runner = {
    createLaunch: async options => {
      if (overrides.preparation) await overrides.preparation;
      if (overrides.launchError) throw new Error(overrides.launchError);
      if (overrides.changeWhilePreparing) doc.version++;
      return { executable: 'solx', args: ['--', options.file], cwd: path.dirname(options.file) };
    },
    startProcess: (launch, write) => {
      launched.push(launch); write('program output');
      return { completion: overrides.completion || Promise.resolve(overrides.result || { code: 0 }), stop: () => { stopped = true; } };
    },
  };
  const file = path.resolve(__dirname, '../src/extension.cjs');
  const module = { exports: {} };
  vm.runInNewContext(fs.readFileSync(file, 'utf8'), { module, require: name => name === 'vscode' ? fake : name === './runner.cjs' ? runner : createRequire(file)(name) }, { filename: file });
  module.exports.activate({ subscriptions: [] });
  return { run: commands.get('solx.runCurrentFile'), stop: commands.get('solx.stopRun'), messages, launched, output, doc, stopped: () => stopped };
}

test('run refuses untrusted, untitled, virtual, and non-Solx documents', async () => {
  for (const overrides of [{ trusted: false }, { document: { isUntitled: true } }, { document: { uri: { scheme: 'git' } } }, { document: { languageId: 'text' } }]) {
    const h = host(overrides);
    assert.equal((await h.run()).status, 'failed');
    assert.equal(h.launched.length, 0);
    assert.equal(h.messages.length, 1);
  }
});

test('stop during launch preparation cancels before starting the interpreter', async () => {
  let ready;
  const preparation = new Promise(resolve => { ready = resolve; });
  const h = host({ preparation });
  const run = h.run();
  await new Promise(resolve => setImmediate(resolve));
  h.stop();
  ready();
  assert.equal((await run).status, 'cancelled');
  assert.equal(h.launched.length, 0);
});

test('dirty file cancellation and failed saves never run stale contents', async () => {
  for (const overrides of [{}, { action: 'Save and Run', document: { save: async () => false } }]) {
    const h = host({ ...overrides, document: { isDirty: true, ...overrides.document } });
    assert.equal((await h.run()).status, 'cancelled');
    assert.equal(h.launched.length, 0);
  }
  const h = host({ document: { isDirty: true }, action: 'Save and Run' });
  assert.equal((await h.run()).status, 'success');
  assert.equal(h.doc.isDirty, false);
  assert.equal(h.launched.length, 1);
});

test('document edits during launch preparation are detected', async () => {
  const h = host({ changeWhilePreparing: true });
  assert.equal((await h.run()).status, 'failed');
  assert.equal(h.launched.length, 0);
  assert.match(h.messages[0], /document changed/);
});

test('run exposes output and reports configuration, launch, and exit failures', async () => {
  for (const options of [{ launchError: 'missing executable' }, { result: { error: new Error('EACCES') } }, { result: { code: 1 } }]) {
    const h = host(options);
    assert.equal((await h.run()).status, 'failed');
    assert.equal(h.messages.length, 1);
  }
  const h = host();
  assert.equal((await h.run()).status, 'success');
  assert.match(h.output.text, /program output/);
  assert.match(h.output.text, /Exited with code 0/);
});

test('only one run starts at a time and the stop command reaches that run', async () => {
  let finish;
  const completion = new Promise(resolve => { finish = resolve; });
  const h = host({ completion });
  const first = h.run();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal((await h.run()).status, 'failed');
  h.stop();
  assert.equal(h.stopped(), true);
  finish({ cancelled: true });
  assert.equal((await first).status, 'cancelled');
  assert.equal(h.launched.length, 1);
});
